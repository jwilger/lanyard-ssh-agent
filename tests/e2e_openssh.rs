//! End-to-end interoperability with real OpenSSH and Git processes.

#![expect(
    clippy::missing_trait_methods,
    reason = "Drop::pin_drop is an unstable implementation detail, not cleanup behavior"
)]
#![expect(
    clippy::too_many_lines,
    reason = "the scenario is intentionally linear so process ownership and evidence stay visible"
)]
#![expect(
    clippy::unseparated_literal_suffix,
    reason = "the strict profile enables mutually exclusive literal-suffix lints"
)]

use std::env;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::OpenOptionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const EXTENSION: u8 = 27;
const EXTENSION_RESPONSE: u8 = 29;
const FAILURE: u8 = 5;
const SIGN_REQUEST: u8 = 13;
const SUCCESS: u8 = 6;

#[derive(Clone, Debug, Eq, PartialEq)]
enum AgentRequest {
    Message(u8),
    Extension(Vec<u8>),
}

enum StartAttempt<T> {
    Started(T),
    Contended(io::Error),
}

fn retry_contended_start<T>(
    maximum_attempts: u8,
    mut attempt: impl FnMut() -> io::Result<StartAttempt<T>>,
) -> io::Result<T> {
    let mut last_contention = None;
    for _attempt_index in 0..maximum_attempts {
        match attempt()? {
            StartAttempt::Started(started) => return Ok(started),
            StartAttempt::Contended(error) => last_contention = Some(error),
        }
    }
    Err(last_contention.unwrap_or_else(|| {
        io::Error::new(ErrorKind::InvalidInput, "at least one start attempt is required")
    }))
}

#[test]
fn retries_when_an_sshd_port_candidate_is_claimed() -> io::Result<()> {
    let mut attempts = 0_u8;

    let selected = retry_contended_start(3, || {
        attempts = attempts.saturating_add(1);
        if attempts == 1 {
            return Ok(StartAttempt::Contended(io::Error::new(
                ErrorKind::AddrInUse,
                "injected loopback port claimant",
            )));
        }
        Ok(StartAttempt::Started(22_u16))
    })?;

    assert_eq!(selected, 22);
    assert_eq!(attempts, 2);
    Ok(())
}

#[test]
#[ignore = "run once through `just e2e`; external processes are too expensive for mutation runs"]
fn authenticates_and_signs_through_lanyard() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let client_key = directory.path().join("client-key");
    let host_key = directory.path().join("host-key");
    generate_key(&client_key)?;
    generate_key(&host_key)?;

    let upstream_socket = directory.path().join("upstream.sock");
    let mut agent = ChildGuard::spawn(
        Command::new("ssh-agent")
            .args(["-D", "-a"])
            .arg(&upstream_socket)
            .stdout(Stdio::null())
            .stderr(Stdio::piped()),
        "ssh-agent",
    )?;
    wait_for_path(&upstream_socket, &mut agent)?;
    run_checked(
        Command::new("ssh-add")
            .arg(&client_key)
            .env("SSH_AUTH_SOCK", &upstream_socket),
        "load the test key into ssh-agent",
    )?;
    fs::remove_file(&client_key)?;

    let recorded_requests = Arc::new(Mutex::new(Vec::new()));
    let recorder_socket = directory.path().join("recorder.sock");
    let recorder = AgentRecorder::start(
        &recorder_socket,
        &upstream_socket,
        Arc::clone(&recorded_requests),
    )?;

    let runtime = directory.path().join("runtime");
    fs::create_dir_all(&runtime)?;
    let lanyard_socket = directory.path().join("lanyard.sock");
    let mut lanyard = ChildGuard::spawn(
        Command::new(env!("CARGO_BIN_EXE_lanyard-ssh-agent"))
            .args(["serve", "--upstream"])
            .arg(&recorder_socket)
            .arg("--socket")
            .arg(&lanyard_socket)
            .env("XDG_RUNTIME_DIR", &runtime)
            .stdout(Stdio::null())
            .stderr(Stdio::piped()),
        "Lanyard",
    )?;
    wait_for_path(&lanyard_socket, &mut lanyard)?;

    query_extensions(&lanyard_socket)?;

    let public_key_path = client_key.with_extension("pub");
    let public_key = fs::read_to_string(&public_key_path)?;
    let authorized_keys = directory.path().join("authorized_keys");
    write_private(&authorized_keys, public_key.as_bytes())?;
    let port = available_port()?;
    let sshd_config = directory.path().join("sshd_config");
    let user = env::var("USER")?;
    write_private(
        &sshd_config,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nPidFile {}\nAllowUsers {user}\n",
            host_key.display(),
            authorized_keys.display(),
            directory.path().join("sshd.pid").display(),
        )
        .as_bytes(),
    )?;
    let sshd_executable = executable("sshd")?;
    let mut sshd = ChildGuard::spawn(
        Command::new(&sshd_executable)
            .args(["-D", "-e", "-f"])
            .arg(&sshd_config)
            .stdout(Stdio::null())
            .stderr(Stdio::piped()),
        "sshd",
    )?;
    wait_for_port(port, &mut sshd)?;

    run_checked(
        Command::new("ssh")
            .args([
                "-F",
                "/dev/null",
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
            ])
            .arg(format!("IdentityAgent={}", lanyard_socket.display()))
            .args(["-i"])
            .arg(&public_key_path)
            .args([
                "-p",
                &port.to_string(),
                &format!("{user}@127.0.0.1"),
                "true",
            ]),
        "authenticate to the test sshd through Lanyard",
    )?;

    let repository = directory.path().join("repository");
    fs::create_dir_all(&repository)?;
    run_git(&repository, ["init", "--quiet"], &lanyard_socket)?;
    run_git(
        &repository,
        ["config", "user.name", "Lanyard End-to-End Test"],
        &lanyard_socket,
    )?;
    run_git(
        &repository,
        ["config", "user.email", "lanyard-e2e@example.invalid"],
        &lanyard_socket,
    )?;
    run_git(
        &repository,
        ["config", "gpg.format", "ssh"],
        &lanyard_socket,
    )?;
    run_git(
        &repository,
        [
            "config",
            "user.signingKey",
            public_key_path
                .to_str()
                .ok_or("public key path is not UTF-8")?,
        ],
        &lanyard_socket,
    )?;
    run_git(
        &repository,
        [
            "commit",
            "--quiet",
            "--allow-empty",
            "-S",
            "-m",
            "signed through Lanyard",
        ],
        &lanyard_socket,
    )?;
    let allowed_signers = directory.path().join("allowed_signers");
    write_private(
        &allowed_signers,
        format!("lanyard-e2e@example.invalid {public_key}").as_bytes(),
    )?;
    run_git(
        &repository,
        [
            "-c",
            &format!("gpg.ssh.allowedSignersFile={}", allowed_signers.display()),
            "verify-commit",
            "HEAD",
        ],
        &lanyard_socket,
    )?;

    let requests = lock(&recorded_requests).clone();
    let query_index = position_extension(&requests, b"query").ok_or("query was not forwarded")?;
    let binding_index = position_extension(&requests, b"session-bind@openssh.com")
        .ok_or("OpenSSH session binding was not forwarded")?;
    let signing_index = requests
        .iter()
        .position(|request| request == &AgentRequest::Message(SIGN_REQUEST))
        .ok_or("no signing request reached ssh-agent")?;
    assert!(query_index < binding_index);
    assert!(binding_index < signing_index);

    drop(recorder);
    Ok(())
}

fn generate_key(path: &Path) -> io::Result<()> {
    run_checked(
        Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path),
        "generate an Ed25519 test key",
    )
}

fn query_extensions(socket: &Path) -> io::Result<()> {
    let mut request = vec![EXTENSION];
    append_string(&mut request, b"query")?;
    let mut stream = UnixStream::connect(socket)?;
    write_frame(&mut stream, &request)?;
    let response = read_frame(&mut stream)?;
    match response.first().copied() {
        Some(FAILURE | SUCCESS) if response.len() == 1 => Ok(()),
        Some(EXTENSION_RESPONSE) if valid_query_response(&response) => Ok(()),
        _ => Err(io::Error::new(
            ErrorKind::InvalidData,
            "Lanyard returned an unsafe query response",
        )),
    }
}

fn valid_query_response(response: &[u8]) -> bool {
    let mut payload = response.get(1..).unwrap_or_default();
    let Ok(name) = read_string(&mut payload) else {
        return false;
    };
    if name != b"query" {
        return false;
    }
    while !payload.is_empty() {
        if read_string(&mut payload).is_err() {
            return false;
        }
    }
    true
}

fn run_git<const N: usize>(
    repository: &Path,
    arguments: [&str; N],
    agent_socket: &Path,
) -> io::Result<()> {
    run_checked(
        Command::new("git")
            .args(arguments)
            .current_dir(repository)
            .env("SSH_AUTH_SOCK", agent_socket)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null"),
        "run Git end-to-end operation",
    )
}

fn executable(name: &str) -> io::Result<PathBuf> {
    let output = checked_output(Command::new("which").arg(name), "locate an executable")?;
    let path = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    Ok(PathBuf::from(path.trim()))
}

fn available_port() -> io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

fn wait_for_path(path: &Path, child: &mut ChildGuard) -> io::Result<()> {
    wait_until(child, || UnixStream::connect(path).is_ok())
}

fn wait_for_port(port: u16, child: &mut ChildGuard) -> io::Result<()> {
    wait_until(child, || TcpStream::connect(("127.0.0.1", port)).is_ok())
}

fn wait_until(child: &mut ChildGuard, ready: impl Fn() -> bool) -> io::Result<()> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or_else(|| io::Error::other("readiness deadline overflowed"))?;
    while Instant::now() < deadline {
        if ready() {
            return Ok(());
        }
        if child.child.try_wait()?.is_some() {
            return Err(child.failure("exited before becoming ready"));
        }
        thread::sleep(Duration::from_millis(20));
    }
    child.terminate()?;
    Err(child.failure("did not become ready"))
}

fn run_checked(command: &mut Command, description: &str) -> io::Result<()> {
    checked_output(command, description).map(|_output| ())
}

fn checked_output(command: &mut Command, description: &str) -> io::Result<Output> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(output);
    }
    Err(io::Error::other(format!(
        "{description} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )))
}

fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents)
}

struct ChildGuard {
    child: Child,
    name: &'static str,
}

impl ChildGuard {
    fn spawn(command: &mut Command, name: &'static str) -> io::Result<Self> {
        Ok(Self {
            child: command.spawn()?,
            name,
        })
    }

    fn failure(&mut self, detail: &str) -> io::Error {
        let stderr = self
            .child
            .stderr
            .as_mut()
            .map(|stream| {
                let mut message = String::new();
                let _read_result = stream.read_to_string(&mut message);
                message
            })
            .unwrap_or_default();
        io::Error::other(format!("{} {detail}\nstderr:\n{stderr}", self.name))
    }

    fn terminate(&mut self) -> io::Result<()> {
        self.child.kill()?;
        let _status = self.child.wait()?;
        Ok(())
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _kill_result = self.child.kill();
        let _wait_result = self.child.wait();
    }
}

struct AgentRecorder {
    shutdown: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
}

impl AgentRecorder {
    fn start(
        socket: &Path,
        upstream: &Path,
        recorded: Arc<Mutex<Vec<AgentRequest>>>,
    ) -> io::Result<Self> {
        let listener = UnixListener::bind(socket)?;
        listener.set_nonblocking(true)?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let listener_shutdown = Arc::clone(&shutdown);
        let upstream_path = upstream.to_path_buf();
        let thread = thread::spawn(move || {
            while !listener_shutdown.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((downstream, _address)) => {
                        let connection_upstream = upstream_path.clone();
                        let connection_recorded = Arc::clone(&recorded);
                        thread::spawn(move || {
                            let _forward_result = record_connection(
                                downstream,
                                &connection_upstream,
                                &connection_recorded,
                            );
                        });
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            shutdown,
            listener: Some(thread),
        })
    }
}

impl Drop for AgentRecorder {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(listener) = self.listener.take() {
            let _join_result = listener.join();
        }
    }
}

fn record_connection(
    mut downstream: UnixStream,
    upstream_path: &Path,
    recorded: &Mutex<Vec<AgentRequest>>,
) -> io::Result<()> {
    let mut upstream = UnixStream::connect(upstream_path)?;
    loop {
        let request = match read_frame(&mut downstream) {
            Ok(frame) => frame,
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error),
        };
        lock(recorded).push(classify(&request));
        write_frame(&mut upstream, &request)?;
        let response = read_frame(&mut upstream)?;
        write_frame(&mut downstream, &response)?;
    }
}

fn classify(request: &[u8]) -> AgentRequest {
    let Some((&message_type, payload)) = request.split_first() else {
        return AgentRequest::Message(0);
    };
    if message_type != EXTENSION {
        return AgentRequest::Message(message_type);
    }
    let mut cursor = payload;
    read_string(&mut cursor).map_or(AgentRequest::Message(message_type), AgentRequest::Extension)
}

fn position_extension(requests: &[AgentRequest], name: &[u8]) -> Option<usize> {
    requests
        .iter()
        .position(|request| request == &AgentRequest::Extension(name.to_vec()))
}

fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut encoded_length = [0u8; 4];
    stream.read_exact(&mut encoded_length)?;
    let frame_length =
        usize::try_from(u32::from_be_bytes(encoded_length)).map_err(io::Error::other)?;
    if frame_length == 0 || frame_length > 256 * 1024 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid agent frame length",
        ));
    }
    let mut body = vec![0; frame_length];
    stream.read_exact(&mut body)?;
    Ok(body)
}

fn write_frame(stream: &mut UnixStream, body: &[u8]) -> io::Result<()> {
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(body)
}

fn append_string(message: &mut Vec<u8>, value: &[u8]) -> io::Result<()> {
    let length = u32::try_from(value.len()).map_err(io::Error::other)?;
    message.extend_from_slice(&length.to_be_bytes());
    message.extend_from_slice(value);
    Ok(())
}

fn read_string(cursor: &mut &[u8]) -> io::Result<Vec<u8>> {
    let encoded = cursor
        .get(..4)
        .ok_or_else(|| io::Error::new(ErrorKind::UnexpectedEof, "missing string length"))?;
    let length = usize::try_from(u32::from_be_bytes(
        encoded.try_into().map_err(io::Error::other)?,
    ))
    .map_err(io::Error::other)?;
    let value = cursor
        .get(4..4usize.saturating_add(length))
        .ok_or_else(|| io::Error::new(ErrorKind::UnexpectedEof, "truncated string"))?;
    *cursor = cursor
        .get(4usize.saturating_add(length)..)
        .ok_or_else(|| io::Error::new(ErrorKind::UnexpectedEof, "truncated string"))?;
    Ok(value.to_vec())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
