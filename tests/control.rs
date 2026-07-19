//! Black-box tests for the local daemon control protocol.

use std::error::Error;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use lanyard_ssh_agent::backend::{Registry, SharedRegistry, Source};
use lanyard_ssh_agent::control::{CandidateStatus, Request, Response, serve};
use tempfile::tempdir;
use tokio::fs;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::{UnixListener, UnixStream};

async fn request(socket: &Path, request: &Request) -> Result<Response, Box<dyn Error>> {
    let mut stream = UnixStream::connect(socket).await?;
    let mut encoded = serde_json::to_vec(request)?;
    encoded.push(b'\n');
    stream.write_all(&encoded).await?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).await?;
    Ok(serde_json::from_str(&response)?)
}

#[tokio::test]
async fn controls_registration_and_reports_stable_status() -> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let socket = root.path().join("control.sock");
    let fallback = root.path().join("onepassword.sock");
    let forwarded = root.path().join("forwarded.sock");
    let discovery_root = root.path().join("discovery");
    let ssh_directory = discovery_root.join("ssh-test");
    fs::create_dir_all(&ssh_directory).await?;
    fs::write(ssh_directory.join("agent.not-a-socket"), b"decoy").await?;
    let _wrong_name_listener = UnixListener::bind(ssh_directory.join("not-an-agent"))?;
    let registry = SharedRegistry::new(Registry::new(fallback.clone()));
    let listener = UnixListener::bind(&socket)?;
    let server = tokio::spawn(serve(listener, registry.clone(), vec![discovery_root]));

    assert_eq!(
        request(
            &socket,
            &Request::Register {
                path: forwarded.clone()
            }
        )
        .await?,
        Response::Updated { changed: true }
    );
    assert_eq!(
        request(&socket, &Request::Status).await?,
        Response::Status {
            candidates: vec![
                CandidateStatus {
                    path: forwarded.clone(),
                    source: Source::Registered,
                    reachable: false,
                },
                CandidateStatus {
                    path: fallback,
                    source: Source::Fallback,
                    reachable: false,
                },
            ]
        }
    );
    assert_eq!(
        request(&socket, &Request::Unregister { path: forwarded }).await?,
        Response::Updated { changed: true }
    );
    assert_eq!(
        request(
            &socket,
            &Request::Unregister {
                path: PathBuf::from("/not/registered")
            }
        )
        .await?,
        Response::Updated { changed: false }
    );

    server.abort();
    Ok(())
}

#[tokio::test]
async fn enforces_the_control_message_boundary_without_mutating_on_overflow()
-> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let socket = root.path().join("control.sock");
    let fallback = root.path().join("fallback.sock");
    let accepted = root.path().join("accepted.sock");
    let injected = root.path().join("injected.sock");
    let registry = SharedRegistry::new(Registry::new(fallback.clone()));
    let listener = UnixListener::bind(&socket)?;
    let server = tokio::spawn(serve(listener, registry, Vec::new()));
    let mut exact = UnixStream::connect(&socket).await?;
    let mut exact_request = serde_json::to_vec(&Request::Register {
        path: accepted.clone(),
    })?;
    exact_request.resize(0x0003_ffff, b' ');
    exact_request.push(b'\n');
    exact.write_all(&exact_request).await?;
    let mut exact_response = String::new();
    BufReader::new(exact).read_line(&mut exact_response).await?;
    assert_eq!(
        serde_json::from_str::<Response>(&exact_response)?,
        Response::Updated { changed: true }
    );

    let mut stream = UnixStream::connect(&socket).await?;
    let mut oversized = serde_json::to_vec(&Request::Register {
        path: injected.clone(),
    })?;
    oversized.resize(0x0004_0001, b' ');
    oversized.push(b'\n');
    stream.write_all(&oversized).await?;
    let mut response = String::new();
    BufReader::new(stream).read_line(&mut response).await?;
    assert!(matches!(
        serde_json::from_str::<Response>(&response)?,
        Response::Error { .. }
    ));

    assert_eq!(
        request(&socket, &Request::Status).await?,
        Response::Status {
            candidates: vec![
                CandidateStatus {
                    path: accepted,
                    source: Source::Registered,
                    reachable: false,
                },
                CandidateStatus {
                    path: fallback,
                    source: Source::Fallback,
                    reachable: false,
                },
            ]
        }
    );
    server.abort();
    Ok(())
}

#[tokio::test]
async fn status_round_trips_non_utf8_unix_socket_paths() -> Result<(), Box<dyn Error>> {
    let root = tempdir()?;
    let socket = root.path().join("control.sock");
    let fallback = root.path().join("fallback.sock");
    let discovery_root = root.path().join("discovery");
    let ssh_directory = discovery_root.join(OsString::from_vec(b"ssh-\xff".to_vec()));
    fs::create_dir_all(&ssh_directory).await?;
    let discovered = ssh_directory.join(OsString::from_vec(b"agent.\xfe".to_vec()));
    let _agent = UnixListener::bind(&discovered)?;
    let registry = SharedRegistry::new(Registry::new(fallback.clone()));
    let listener = UnixListener::bind(&socket)?;
    let server = tokio::spawn(serve(listener, registry, vec![discovery_root]));

    assert_eq!(
        request(&socket, &Request::Status).await?,
        Response::Status {
            candidates: vec![
                CandidateStatus {
                    path: discovered,
                    source: Source::Discovered,
                    reachable: true,
                },
                CandidateStatus {
                    path: fallback,
                    source: Source::Fallback,
                    reachable: false,
                },
            ]
        }
    );

    server.abort();
    Ok(())
}

#[test]
fn protocol_has_stable_tagged_json() -> Result<(), Box<dyn Error>> {
    assert_eq!(
        serde_json::to_string(&Request::Register {
            path: PathBuf::from("/tmp/agent.sock")
        })?,
        r#"{"command":"register","path":"/tmp/agent.sock"}"#
    );
    Ok(())
}
