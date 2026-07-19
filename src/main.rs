//! Command-line entry point for Lanyard.

use std::env;
use std::error::Error;
use std::fs::{File, OpenOptions, Permissions, TryLockError};
use std::io::{self, ErrorKind};
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use lanyard_ssh_agent::paths::{agent_socket, control_socket};
use lanyard_ssh_agent::proxy::{Config, serve};
use tokio::fs::{create_dir_all, remove_file, set_permissions, symlink_metadata};
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
            let listening_socket = requested_socket.map_or_else(default_agent_socket, Ok)?;
            run_proxy(&listening_socket, upstream).await?;
        }
        Command::Register { socket } => println!("register {}", socket.display()),
        Command::Unregister { socket } => println!("unregister {}", socket.display()),
        Command::Status { json } => println!("status json={json}"),
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

async fn run_proxy(socket: &Path, upstream: PathBuf) -> io::Result<()> {
    prepare_socket_parent(socket).await?;
    let _instance_lock = acquire_instance_lock(socket)?;
    let listener = bind_owned_socket(socket).await?;
    set_permissions(socket, Permissions::from_mode(0o600)).await?;

    let result = tokio::select! {
        proxy_result = serve(listener, Config::new(upstream)) => proxy_result,
        signal_result = shutdown_signal() => signal_result,
    };
    let cleanup_result = remove_file(socket).await;
    match (result, cleanup_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) if error.kind() != ErrorKind::NotFound => Err(error),
        _ => Ok(()),
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

fn default_agent_socket() -> io::Result<PathBuf> {
    runtime_directory().map(|runtime| agent_socket(&runtime))
}

fn runtime_directory() -> io::Result<PathBuf> {
    env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set"))
}
