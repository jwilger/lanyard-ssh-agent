//! Black-box tests for the public command-line surface.

use std::error::Error;
use std::fs;
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_exposes_the_public_command_surface() {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("lanyard-ssh-agent"));
    command.arg("--help");

    command.assert().success().stdout(
        predicate::str::contains("serve")
            .and(predicate::str::contains("register"))
            .and(predicate::str::contains("unregister"))
            .and(predicate::str::contains("status"))
            .and(predicate::str::contains("socket")),
    );
}

#[test]
fn socket_prints_the_stable_xdg_agent_path() {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("lanyard-ssh-agent"));
    command
        .env("XDG_RUNTIME_DIR", "/run/user/1234")
        .arg("socket");

    command
        .assert()
        .success()
        .stdout("/run/user/1234/lanyard-ssh-agent/agent.sock\n");
}

#[test]
fn serve_owns_a_private_stable_socket_and_refuses_a_second_instance() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::tempdir()?;
    let runtime = directory.path().join("runtime");
    let upstream = directory.path().join("upstream.sock");
    fs::create_dir_all(&runtime)?;
    let _upstream_listener = UnixListener::bind(&upstream)?;
    let binary = assert_cmd::cargo::cargo_bin!("lanyard-ssh-agent");
    let mut daemon = ProcessCommand::new(binary)
        .env("XDG_RUNTIME_DIR", &runtime)
        .args(["serve", "--upstream"])
        .arg(&upstream)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let socket = runtime.join("lanyard-ssh-agent/agent.sock");
    wait_for_path(&socket)?;

    let parent_mode = fs::metadata(socket.parent().ok_or("socket has no parent")?)?
        .permissions()
        .mode()
        & 0o777;
    let socket_metadata = fs::metadata(&socket)?;
    assert_eq!(parent_mode, 0o700);
    assert_eq!(socket_metadata.permissions().mode() & 0o777, 0o600);
    assert!(socket_metadata.file_type().is_socket());

    let second = ProcessCommand::new(binary)
        .env("XDG_RUNTIME_DIR", &runtime)
        .args(["serve", "--upstream"])
        .arg(&upstream)
        .output()?;
    assert!(!second.status.success());
    let _original_socket = UnixStream::connect(&socket)?;

    let signal_status = ProcessCommand::new("kill")
        .args(["-TERM", &daemon.id().to_string()])
        .status()?;
    assert!(signal_status.success());
    assert!(daemon.wait()?.success());
    assert!(!socket.exists());
    Ok(())
}

#[test]
fn serve_recovers_a_stale_socket() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let runtime = directory.path().join("runtime");
    let upstream = directory.path().join("upstream.sock");
    let socket_directory = runtime.join("lanyard-ssh-agent");
    let socket = socket_directory.join("agent.sock");
    fs::create_dir_all(&socket_directory)?;
    let stale_listener = UnixListener::bind(&socket)?;
    drop(stale_listener);
    let _upstream_listener = UnixListener::bind(&upstream)?;
    let binary = assert_cmd::cargo::cargo_bin!("lanyard-ssh-agent");
    let mut daemon = ProcessCommand::new(binary)
        .env("XDG_RUNTIME_DIR", &runtime)
        .args(["serve", "--upstream"])
        .arg(&upstream)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    wait_for_connectable_socket(&socket)?;
    let signal_status = ProcessCommand::new("kill")
        .args(["-TERM", &daemon.id().to_string()])
        .status()?;
    assert!(signal_status.success());
    assert!(daemon.wait()?.success());
    assert!(!socket.exists());
    Ok(())
}

fn wait_for_path(path: &Path) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(2))
        .ok_or("deadline overflowed")?;
    while !path.exists() {
        if Instant::now() >= deadline {
            return Err("daemon did not create its socket".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn wait_for_connectable_socket(path: &Path) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(2))
        .ok_or("deadline overflowed")?;
    loop {
        if UnixStream::connect(path).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("daemon did not replace the stale socket".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
