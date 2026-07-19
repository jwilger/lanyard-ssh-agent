//! Command-line entry point for Lanyard.

use std::env;
use std::error::Error;
use std::fs::{File, OpenOptions, Permissions, TryLockError};
use std::io::{self, ErrorKind};
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use lanyard_ssh_agent::backend::Source;
use lanyard_ssh_agent::control::{CandidateStatus, Request, Response, serve as serve_control};
use lanyard_ssh_agent::paths::{agent_socket, control_socket};
use lanyard_ssh_agent::proxy::{Config, serve};
use tokio::fs::{create_dir_all, remove_file, set_permissions, symlink_metadata};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::ctrl_c;
use tokio::signal::unix::{SignalKind, signal};

#[derive(Debug, Parser)]
#[command(name = "lanyard-ssh-agent", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the SSH agent proxy.
    Serve {
        /// Upstream SSH agent to expose through Lanyard.
        #[arg(long, value_name = "PATH")]
        upstream: PathBuf,
        /// Override the stable listening socket (primarily for testing).
        #[arg(long, value_name = "PATH")]
        socket: Option<PathBuf>,
    },
    /// Register an upstream SSH agent socket.
    Register { socket: PathBuf },
    /// Remove a previously registered upstream socket.
    Unregister { socket: PathBuf },
    /// Show the daemon and upstream-agent state.
    Status {
        /// Emit stable JSON suitable for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Print a Lanyard socket path.
    Socket {
        /// Print the control socket instead of the SSH agent socket.
        #[arg(long)]
        control: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve {
            upstream,
            socket: requested_socket,
        } => {
            let runtime = runtime_directory()?;
            let listening_socket = requested_socket.unwrap_or_else(|| agent_socket(&runtime));
            run_proxy(&listening_socket, &control_socket(&runtime), upstream).await?;
        }
        Command::Register { socket } => {
            expect_update(send_control(Request::Register { path: socket }).await?)?;
        }
        Command::Unregister { socket } => {
            expect_update(send_control(Request::Unregister { path: socket }).await?)?;
        }
        Command::Status { json } => print_status(send_control(Request::Status).await?, json)?,
        Command::Socket { control } => {
            let runtime_directory = runtime_directory()?;
            let socket = if control {
                control_socket(&runtime_directory)
            } else {
                agent_socket(&runtime_directory)
            };
            println!("{}", socket.display());
        }
    }
    Ok(())
}

async fn run_proxy(socket: &Path, control_path: &Path, upstream: PathBuf) -> io::Result<()> {
    prepare_socket_parent(socket).await?;
    prepare_socket_parent(control_path).await?;
    let _instance_lock = acquire_instance_lock(socket)?;
    let listener = bind_owned_socket(socket).await?;
    let control_listener = match bind_owned_socket(control_path).await {
        Ok(bound_control_listener) => bound_control_listener,
        Err(error) => {
            let _cleanup_result = remove_file(socket).await;
            return Err(error);
        }
    };
    if let Err(error) = set_permissions(socket, Permissions::from_mode(0o600)).await {
        cleanup_startup_sockets(socket, control_path).await;
        return Err(error);
    }
    if let Err(error) = set_permissions(control_path, Permissions::from_mode(0o600)).await {
        cleanup_startup_sockets(socket, control_path).await;
        return Err(error);
    }

    let config = Config::new(upstream);
    let registry = config.registry();

    let result = tokio::select! {
        proxy_result = serve(listener, config) => proxy_result,
        control_result = serve_control(control_listener, registry, vec![PathBuf::from("/tmp")]) => control_result,
        signal_result = shutdown_signal() => signal_result,
    };
    let cleanup_result = remove_file(socket).await;
    let control_cleanup_result = remove_file(control_path).await;
    match (result, cleanup_result, control_cleanup_result) {
        (Err(error), _, _) => Err(error),
        (Ok(()), Err(error), _) | (Ok(()), _, Err(error))
            if error.kind() != ErrorKind::NotFound =>
        {
            Err(error)
        }
        _ => Ok(()),
    }
}

async fn cleanup_startup_sockets(agent: &Path, control: &Path) {
    let _agent_cleanup = remove_file(agent).await;
    let _control_cleanup = remove_file(control).await;
}

async fn send_control(request: Request) -> io::Result<Response> {
    let runtime = runtime_directory()?;
    let mut stream = UnixStream::connect(control_socket(&runtime)).await?;
    let mut request_bytes = serde_json::to_vec(&request).map_err(io::Error::other)?;
    request_bytes.push(b'\n');
    stream.write_all(&request_bytes).await?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).await?;
    serde_json::from_str(&response).map_err(io::Error::other)
}

fn expect_update(response: Response) -> io::Result<()> {
    match response {
        Response::Updated { .. } => Ok(()),
        Response::Error { message } => Err(io::Error::other(message)),
        Response::Status { .. } => Err(io::Error::new(
            ErrorKind::InvalidData,
            "daemon returned status for a mutation",
        )),
    }
}

fn print_status(response: Response, json: bool) -> io::Result<()> {
    match response {
        status_response @ Response::Status { .. } if json => {
            println!(
                "{}",
                serde_json::to_string(&status_response).map_err(io::Error::other)?
            );
            Ok(())
        }
        Response::Status { candidates } => {
            for CandidateStatus {
                path,
                source,
                reachable,
            } in candidates
            {
                let source_name = match source {
                    Source::Registered => "registered",
                    Source::Discovered => "discovered",
                    Source::Fallback => "fallback",
                };
                println!(
                    "{source_name}\t{}\t{}",
                    if reachable { "ready" } else { "down" },
                    path.display()
                );
            }
            Ok(())
        }
        Response::Error { message } => Err(io::Error::other(message)),
        Response::Updated { .. } => Err(io::Error::new(
            ErrorKind::InvalidData,
            "daemon returned mutation result for status",
        )),
    }
}

fn acquire_instance_lock(socket: &Path) -> io::Result<File> {
    let lock_path = socket.with_file_name("daemon.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)?;
    lock_file.set_permissions(Permissions::from_mode(0o600))?;
    match lock_file.try_lock() {
        Ok(()) => Ok(lock_file),
        Err(TryLockError::WouldBlock) => Err(io::Error::new(
            ErrorKind::AddrInUse,
            "another Lanyard instance owns the runtime directory",
        )),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

async fn bind_owned_socket(socket: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(socket) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            if UnixStream::connect(socket).await.is_ok() {
                return Err(io::Error::new(
                    ErrorKind::AddrInUse,
                    "another Lanyard-compatible agent is already listening",
                ));
            }
            remove_stale_socket(socket).await?;
            UnixListener::bind(socket)
        }
        Err(error) => Err(error),
    }
}

async fn shutdown_signal() -> io::Result<()> {
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        interrupt_result = ctrl_c() => interrupt_result,
        _termination = terminate.recv() => Ok(()),
    }
}

async fn prepare_socket_parent(socket: &Path) -> io::Result<()> {
    let parent = socket.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "agent socket has no parent directory",
        )
    })?;
    create_dir_all(parent).await?;
    set_permissions(parent, Permissions::from_mode(0o700)).await
}

async fn remove_stale_socket(socket: &Path) -> io::Result<()> {
    match symlink_metadata(socket).await {
        Ok(metadata) if metadata.file_type().is_socket() => remove_file(socket).await,
        Ok(_) => Err(io::Error::new(
            ErrorKind::AlreadyExists,
            "refusing to replace a non-socket path",
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn runtime_directory() -> io::Result<PathBuf> {
    env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))
}
