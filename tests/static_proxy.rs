//! Black-box protocol tests for the single-upstream proxy.

use std::error::Error;
use std::io;
use std::path::Path;
use std::time::Duration;

use lanyard_ssh_agent::proxy::{Config, Timeouts, serve};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::{sleep, timeout};

const REQUEST_IDENTITIES: &[u8] = &[11];
const IDENTITIES_ANSWER: &[u8] = &[12, 0, 0, 0, 0];
const ADD_IDENTITY: &[u8] = &[17];
const FAILURE: &[u8] = &[5];
const SIGN_REQUEST: &[u8] = &[13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const SIGN_RESPONSE: &[u8] = &[14, 0, 0, 0, 3, b's', b'i', b'g'];
const QUERY_EXTENSION: &[u8] = &[27, 0, 0, 0, 5, b'q', b'u', b'e', b'r', b'y'];
const EXTENSION_RESPONSE: &[u8] = &[6];
const SESSION_BIND_EXTENSION: &[u8] = &[
    27, 0, 0, 0, 24, b's', b'e', b's', b's', b'i', b'o', b'n', b'-', b'b', b'i', b'n', b'd', b'@',
    b'o', b'p', b'e', b'n', b's', b's', b'h', b'.', b'c', b'o', b'm',
];
const UNKNOWN_EXTENSION: &[u8] = &[27, 0, 0, 0, 7, b'u', b'n', b'k', b'n', b'o', b'w', b'n'];

#[tokio::test]
async fn proxies_read_only_agent_operations_and_rejects_mutation() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let upstream_path = directory.path().join("upstream.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let upstream_listener = UnixListener::bind(&upstream_path)?;
    let lanyard_listener = UnixListener::bind(&lanyard_path)?;

    let fake_agent = tokio::spawn(async move {
        let (mut stream, _) = upstream_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, REQUEST_IDENTITIES);
        write_frame(&mut stream, IDENTITIES_ANSWER).await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, SIGN_RESPONSE).await?;
        assert_eq!(read_frame(&mut stream).await?, QUERY_EXTENSION);
        write_frame(&mut stream, EXTENSION_RESPONSE).await?;
        assert_eq!(read_frame(&mut stream).await?, SESSION_BIND_EXTENSION);
        write_frame(&mut stream, EXTENSION_RESPONSE).await?;
        Ok::<(), io::Error>(())
    });

    let proxy = tokio::spawn(serve(lanyard_listener, Config::new(upstream_path)));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, ADD_IDENTITY).await?;
    assert_eq!(read_frame(&mut client).await?, FAILURE);

    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    assert_eq!(read_frame(&mut client).await?, IDENTITIES_ANSWER);

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(read_frame(&mut client).await?, SIGN_RESPONSE);

    write_frame(&mut client, UNKNOWN_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, FAILURE);

    write_frame(&mut client, QUERY_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, EXTENSION_RESPONSE);

    write_frame(&mut client, SESSION_BIND_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, EXTENSION_RESPONSE);

    fake_agent.await??;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn reconnects_an_idempotent_request_after_the_upstream_restarts() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::tempdir()?;
    let upstream_path = directory.path().join("upstream.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let upstream_listener = UnixListener::bind(&upstream_path)?;
    let lanyard_listener = UnixListener::bind(&lanyard_path)?;
    let fake_agent = tokio::spawn(async move {
        let (mut first, _) = timeout(Duration::from_secs(1), upstream_listener.accept()).await??;
        assert_eq!(read_frame(&mut first).await?, REQUEST_IDENTITIES);
        write_frame(&mut first, IDENTITIES_ANSWER).await?;
        drop(first);
        let (mut second, _) = upstream_listener.accept().await?;
        assert_eq!(read_frame(&mut second).await?, REQUEST_IDENTITIES);
        write_frame(&mut second, IDENTITIES_ANSWER).await?;
        Ok::<(), io::Error>(())
    });
    let proxy = tokio::spawn(serve(lanyard_listener, Config::new(upstream_path)));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    assert_eq!(read_frame(&mut client).await?, IDENTITIES_ANSWER);
    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    assert_eq!(read_frame(&mut client).await?, IDENTITIES_ANSWER);

    fake_agent.await??;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn replays_successful_session_bindings_before_using_a_reconnected_upstream()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let upstream_path = directory.path().join("upstream.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let upstream_listener = UnixListener::bind(&upstream_path)?;
    let lanyard_listener = UnixListener::bind(&lanyard_path)?;
    let fake_agent = tokio::spawn(async move {
        let (mut first, _) = upstream_listener.accept().await?;
        assert_eq!(read_frame(&mut first).await?, SESSION_BIND_EXTENSION);
        write_frame(&mut first, EXTENSION_RESPONSE).await?;
        drop(first);

        let (mut second, _) = upstream_listener.accept().await?;
        assert_eq!(read_frame(&mut second).await?, SESSION_BIND_EXTENSION);
        write_frame(&mut second, EXTENSION_RESPONSE).await?;
        assert_eq!(read_frame(&mut second).await?, REQUEST_IDENTITIES);
        write_frame(&mut second, IDENTITIES_ANSWER).await?;
        Ok::<(), io::Error>(())
    });
    let proxy = tokio::spawn(serve(lanyard_listener, Config::new(upstream_path)));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SESSION_BIND_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, EXTENSION_RESPONSE);
    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    assert_eq!(read_frame(&mut client).await?, IDENTITIES_ANSWER);

    fake_agent.await??;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn does_not_retry_an_ambiguous_signing_failure_on_the_same_backend()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let upstream_path = directory.path().join("upstream.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let upstream_listener = UnixListener::bind(&upstream_path)?;
    let lanyard_listener = UnixListener::bind(&lanyard_path)?;
    let fake_agent = tokio::spawn(async move {
        let (mut first, _) = timeout(Duration::from_secs(1), upstream_listener.accept()).await??;
        assert_eq!(read_frame(&mut first).await?, SIGN_REQUEST);
        drop(first);
        let retry = timeout(Duration::from_millis(200), upstream_listener.accept()).await;
        if retry.is_ok() {
            return Err(io::Error::other("signing request was retried"));
        }
        Ok::<(), io::Error>(())
    });
    let proxy = tokio::spawn(serve(lanyard_listener, Config::new(upstream_path)));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(read_frame(&mut client).await?, FAILURE);

    fake_agent.await??;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn closes_a_connection_that_exceeds_the_message_limit() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let upstream_path = directory.path().join("upstream.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let _upstream_listener = UnixListener::bind(&upstream_path)?;
    let lanyard_listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(lanyard_listener, Config::new(upstream_path)));
    for invalid_length in [0, (256 * 1024) + 1] {
        let mut client = UnixStream::connect(&lanyard_path).await?;
        client.write_u32(invalid_length).await?;
        client.shutdown().await?;
        let mut response = [u8::default(); 1];
        assert_eq!(client.read(&mut response).await?, 0);
    }

    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn bounds_the_complete_exchange_with_an_unresponsive_upstream() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::tempdir()?;
    let upstream_path = directory.path().join("upstream.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let upstream_listener = UnixListener::bind(&upstream_path)?;
    let lanyard_listener = UnixListener::bind(&lanyard_path)?;
    let timeouts = Timeouts::new(
        Duration::from_millis(50),
        Duration::from_millis(50),
        Duration::from_millis(50),
    );
    let proxy = tokio::spawn(serve(
        lanyard_listener,
        Config::new(upstream_path).with_timeouts(timeouts),
    ));
    let holding_agent = tokio::spawn(async move {
        let (_stream, _) = upstream_listener.accept().await?;
        sleep(Duration::from_secs(1)).await;
        Ok::<(), io::Error>(())
    });
    let mut client = UnixStream::connect(&lanyard_path).await?;
    let mut request = vec![0; 256 * 1024];
    let first_byte = request.first_mut().ok_or("request has no message type")?;
    *first_byte = 13;

    write_frame(&mut client, &request).await?;
    let response = timeout(Duration::from_millis(500), read_frame(&mut client)).await??;
    assert_eq!(response, FAILURE);

    holding_agent.abort();
    proxy.abort();
    Ok(())
}

async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    timeout(Duration::from_secs(1), read_frame_unbounded(stream))
        .await
        .map_err(|_elapsed| io::Error::new(io::ErrorKind::TimedOut, "test frame timed out"))?
}

async fn read_frame_unbounded(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
    let length = stream.read_u32().await?;
    let mut body = vec![0; usize::try_from(length).unwrap_or(usize::MAX)];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

async fn write_frame(stream: &mut (impl AsyncWrite + Unpin), body: &[u8]) -> io::Result<()> {
    stream
        .write_u32(u32::try_from(body.len()).unwrap_or(u32::MAX))
        .await?;
    stream.write_all(body).await
}

#[test]
fn proxy_config_uses_documented_protocol_bounds() {
    let config = Config::new(Path::new("/tmp/upstream.sock"));

    assert_eq!(config.maximum_message_bytes(), 256 * 1024);
    assert_eq!(config.maximum_candidates(), 32);
    assert_eq!(config.timeouts().connect(), Duration::from_millis(500));
    assert_eq!(config.timeouts().identities(), Duration::from_secs(2));
    assert_eq!(config.timeouts().sign(), Duration::from_secs(30));
}
