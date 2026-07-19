//! Black-box tests for multi-agent identity aggregation.

use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use lanyard_ssh_agent::backend::{Backend, Registry, Source};
use lanyard_ssh_agent::proxy::{Config, Timeouts, serve};
use tokio::fs;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

const REQUEST_IDENTITIES: &[u8] = &[11];
const SIGN_REQUEST: &[u8] = &[13, 0, 0, 0, 1, b'k', 0, 0, 0, 0, 0, 0, 0, 0];
const OTHER_KEY_SIGN_REQUEST: &[u8] = &[13, 0, 0, 0, 1, b'q', 0, 0, 0, 0, 0, 0, 0, 0];
const SESSION_BIND: &[u8] = &[
    27, 0, 0, 0, 24, b's', b'e', b's', b's', b'i', b'o', b'n', b'-', b'b', b'i', b'n', b'd', b'@',
    b'o', b'p', b'e', b'n', b's', b's', b'h', b'.', b'c', b'o', b'm', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0,
];
const QUERY_EXTENSION: &[u8] = &[27, 0, 0, 0, 5, b'q', b'u', b'e', b'r', b'y'];

#[test]
fn registry_prefers_recent_registrations_and_reserves_the_fallback() {
    let mut registry = Registry::new(PathBuf::from("/fallback"));
    let registration_count: usize = 40;
    for index in 0..registration_count {
        registry.register(PathBuf::from(format!("/registered/{index}")));
    }

    let candidates = registry.candidates(Vec::new());

    assert_eq!(candidates.len(), 32);
    assert_eq!(
        candidates.first().map(Backend::path),
        Some(PathBuf::from("/registered/39").as_path())
    );
    assert_eq!(
        candidates.last().map(Backend::source),
        Some(Source::Fallback)
    );
    assert_eq!(
        candidates.last().map(Backend::path),
        Some(PathBuf::from("/fallback").as_path())
    );
}

#[test]
fn signer_promotion_changes_only_signing_order() {
    let fallback = PathBuf::from("/fallback");
    let first = PathBuf::from("/registered/first");
    let second = PathBuf::from("/registered/second");
    let mut registry = Registry::new(fallback.clone());
    registry.register(first.clone());
    registry.register(second.clone());

    registry.promote_signer(b"key-a".to_vec(), fallback.clone());

    assert_eq!(
        registry
            .candidates(Vec::new())
            .iter()
            .map(Backend::path)
            .collect::<Vec<_>>(),
        [&second, &first, &fallback]
    );
    assert_eq!(
        registry
            .signing_candidates(Vec::new(), b"key-a")
            .iter()
            .map(Backend::path)
            .collect::<Vec<_>>(),
        [&fallback, &second, &first]
    );
    assert_eq!(
        registry
            .signing_candidates(Vec::new(), b"key-b")
            .iter()
            .map(|backend| backend.path().to_path_buf())
            .collect::<Vec<_>>(),
        vec![second, first, fallback]
    );
}

#[test]
fn refreshing_an_affinity_does_not_consume_another_cache_slot() {
    let fallback = PathBuf::from("/fallback");
    let registered = PathBuf::from("/registered");
    let mut registry = Registry::new(fallback.clone());
    registry.register(registered.clone());
    registry.promote_signer(b"oldest".to_vec(), fallback.clone());
    for key in [
        "00", "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14",
        "15", "16", "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29",
        "30",
    ] {
        registry.promote_signer(key.as_bytes().to_vec(), registered.clone());
    }

    registry.promote_signer(b"00".to_vec(), registered.clone());

    assert_eq!(
        registry
            .signing_candidates(Vec::new(), b"oldest")
            .first()
            .map(Backend::path),
        Some(fallback.as_path())
    );

    registry.promote_signer(b"31".to_vec(), registered.clone());

    assert_eq!(
        registry
            .signing_candidates(Vec::new(), b"oldest")
            .first()
            .map(Backend::path),
        Some(registered.as_path())
    );
}

#[tokio::test]
async fn returns_a_deduplicated_union_from_registered_discovered_and_fallback_agents()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let registered_path = directory.path().join("registered.sock");
    let malformed_path = directory.path().join("malformed.sock");
    let discovery_root = directory.path().join("discovery");
    let forwarded_directory = discovery_root.join("ssh-forwarded");
    let discovered_path = forwarded_directory.join("agent.42");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    fs::create_dir_all(&forwarded_directory).await?;

    let registered = fake_agent(
        UnixListener::bind(&registered_path)?,
        identities_response(&[(b"key-one", b"registered")]),
    );
    let malformed = fake_agent(UnixListener::bind(&malformed_path)?, vec![12, 0, 0]);
    let discovered = fake_agent(
        UnixListener::bind(&discovered_path)?,
        identities_response(&[(b"key-one", b"duplicate"), (b"key-two", b"forwarded")]),
    );
    let fallback = fake_agent(
        UnixListener::bind(&fallback_path)?,
        identities_response(&[(b"key-three", b"fallback")]),
    );
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path)
            .with_registered(vec![malformed_path, registered_path])
            .with_discovery_roots(vec![discovery_root]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    let response = read_frame(&mut client).await?;
    assert_eq!(
        response,
        identities_response(&[
            (b"key-one", b"registered"),
            (b"key-two", b"forwarded"),
            (b"key-three", b"fallback"),
        ])
    );

    timeout(Duration::from_secs(1), registered).await???;
    timeout(Duration::from_secs(1), malformed).await???;
    timeout(Duration::from_secs(1), discovered).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn signing_prefers_a_discovered_forwarded_agent_before_fallback() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::tempdir()?;
    let discovery_root = directory.path().join("discovery");
    let forwarded_directory = discovery_root.join("ssh-forwarded");
    let forwarded_path = forwarded_directory.join("agent.42");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    fs::create_dir_all(&forwarded_directory).await?;
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[5]).await
    });
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_discovery_roots(vec![discovery_root]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(
        read_frame(&mut client).await?,
        [14, 0, 0, 0, 3, b's', b'i', b'g']
    );

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn signing_advances_after_a_candidate_response_timeout() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let stalled_path = directory.path().join("stalled.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let stalled_listener = UnixListener::bind(&stalled_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let stalled = tokio::spawn(async move {
        let (mut stream, _) = stalled_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        sleep(Duration::from_millis(100)).await;
        Ok::<(), io::Error>(())
    });
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path)
            .with_registered(vec![stalled_path])
            .with_timeouts(Timeouts::new(
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_millis(20),
            )),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(
        read_frame(&mut client).await?,
        [14, 0, 0, 0, 3, b's', b'i', b'g']
    );

    timeout(Duration::from_secs(1), stalled).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn successful_signing_promotes_that_backend_for_later_signatures()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[5]).await?;
        assert!(
            timeout(Duration::from_millis(200), forwarded_listener.accept())
                .await
                .is_err(),
            "the failed first candidate should be bypassed after signer promotion"
        );
        Ok::<(), io::Error>(())
    });
    let fallback = tokio::spawn(async move {
        for () in [(), ()] {
            let (mut stream, _) = fallback_listener.accept().await?;
            assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
            write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await?;
        }
        Ok::<(), io::Error>(())
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    for () in [(), ()] {
        write_frame(&mut client, SIGN_REQUEST).await?;
        assert_eq!(
            read_frame(&mut client).await?,
            [14, 0, 0, 0, 3, b's', b'i', b'g']
        );
    }

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn signer_promotion_is_scoped_to_the_requested_key() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut first, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut first).await?, SIGN_REQUEST);
        write_frame(&mut first, &[5]).await?;
        let (mut second, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut second).await?, OTHER_KEY_SIGN_REQUEST);
        write_frame(&mut second, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await
    });
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await?;
        assert!(
            timeout(Duration::from_millis(200), fallback_listener.accept())
                .await
                .is_err(),
            "a signer promoted for one key must not be tried first for another key"
        );
        Ok::<(), io::Error>(())
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    for request in [SIGN_REQUEST, OTHER_KEY_SIGN_REQUEST] {
        write_frame(&mut client, request).await?;
        assert_eq!(
            read_frame(&mut client).await?,
            [14, 0, 0, 0, 3, b's', b'i', b'g']
        );
    }

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn signing_fails_closed_when_every_candidate_rejects() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded = fake_request(UnixListener::bind(&forwarded_path)?, SIGN_REQUEST, vec![5]);
    let fallback = fake_request(UnixListener::bind(&fallback_path)?, SIGN_REQUEST, vec![5]);
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(read_frame(&mut client).await?, [5]);

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn signing_skips_a_candidate_with_the_wrong_response_type() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[6]).await
    });
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(
        read_frame(&mut client).await?,
        [14, 0, 0, 0, 3, b's', b'i', b'g']
    );

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn query_skips_an_explicit_extension_failure() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, QUERY_EXTENSION);
        write_frame(&mut stream, &[28]).await
    });
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, QUERY_EXTENSION);
        write_frame(&mut stream, &[6]).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, QUERY_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, [6]);

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn query_accepts_a_valid_extension_response() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let response = query_response(&[b"session-bind@openssh.com"]);
    let fallback = fake_request(
        UnixListener::bind(&fallback_path)?,
        QUERY_EXTENSION,
        response.clone(),
    );
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(listener, Config::new(fallback_path)));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, QUERY_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, response);

    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn signing_advances_to_a_different_candidate_after_an_ambiguous_disconnect()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        Ok::<(), io::Error>(())
    });
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(
        read_frame(&mut client).await?,
        [14, 0, 0, 0, 3, b's', b'i', b'g']
    );

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn session_binding_fails_closed_after_an_ambiguous_disconnect() -> Result<(), Box<dyn Error>>
{
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SESSION_BIND);
        Ok::<(), io::Error>(())
    });
    let fallback = tokio::spawn(async move {
        assert!(
            timeout(Duration::from_millis(200), fallback_listener.accept())
                .await
                .is_err(),
            "an ambiguous session binding must not reach the fallback"
        );
        Ok::<(), io::Error>(())
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SESSION_BIND).await?;
    assert_eq!(read_frame(&mut client).await?, [5]);

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn session_binding_fails_closed_after_an_ambiguous_response() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SESSION_BIND);
        write_frame(&mut stream, &[14, 0, 0, 0, 1, b'x']).await
    });
    let fallback = tokio::spawn(async move {
        assert!(
            timeout(Duration::from_millis(200), fallback_listener.accept())
                .await
                .is_err(),
            "an ambiguous session-binding response must not reach the fallback"
        );
        Ok::<(), io::Error>(())
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SESSION_BIND).await?;
    assert_eq!(read_frame(&mut client).await?, [5]);

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn identities_fail_when_no_backend_returns_a_valid_answer() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let malformed_path = directory.path().join("malformed.sock");
    let unavailable_path = directory.path().join("unavailable.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let malformed = fake_agent(UnixListener::bind(&malformed_path)?, vec![12, 0, 0]);
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(unavailable_path).with_registered(vec![malformed_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    assert_eq!(read_frame(&mut client).await?, [5]);

    timeout(Duration::from_secs(1), malformed).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn identity_union_fails_when_the_synthesized_response_exceeds_the_limit()
-> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let first_path = directory.path().join("first.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let large_key_a = vec![b'a'; 140_000];
    let large_key_b = vec![b'b'; 140_000];
    let first = fake_agent(
        UnixListener::bind(&first_path)?,
        identities_response(&[(&large_key_a, b"first")]),
    );
    let fallback = fake_agent(
        UnixListener::bind(&fallback_path)?,
        identities_response(&[(&large_key_b, b"fallback")]),
    );
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![first_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, REQUEST_IDENTITIES).await?;
    assert_eq!(read_frame(&mut client).await?, [5]);

    timeout(Duration::from_secs(1), first).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn session_binding_does_not_pin_signing_to_the_first_agent() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let forwarded_path = directory.path().join("forwarded.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let forwarded_listener = UnixListener::bind(&forwarded_path)?;
    let fallback_listener = UnixListener::bind(&fallback_path)?;
    let forwarded = tokio::spawn(async move {
        let (mut binding_stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut binding_stream).await?, SESSION_BIND);
        write_frame(&mut binding_stream, &[6]).await?;

        let (mut signing_stream, _) = forwarded_listener.accept().await?;
        assert_eq!(read_frame(&mut signing_stream).await?, SESSION_BIND);
        write_frame(&mut signing_stream, &[6]).await?;
        assert_eq!(read_frame(&mut signing_stream).await?, SIGN_REQUEST);
        write_frame(&mut signing_stream, &[5]).await
    });
    let fallback = tokio::spawn(async move {
        let (mut stream, _) = fallback_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, SESSION_BIND);
        write_frame(&mut stream, &[6]).await?;
        assert_eq!(read_frame(&mut stream).await?, SIGN_REQUEST);
        write_frame(&mut stream, &[14, 0, 0, 0, 3, b's', b'i', b'g']).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![forwarded_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, SESSION_BIND).await?;
    assert_eq!(read_frame(&mut client).await?, [6]);
    write_frame(&mut client, SIGN_REQUEST).await?;
    assert_eq!(
        read_frame(&mut client).await?,
        [14, 0, 0, 0, 3, b's', b'i', b'g']
    );

    timeout(Duration::from_secs(1), forwarded).await???;
    timeout(Duration::from_secs(1), fallback).await???;
    proxy.abort();
    Ok(())
}

#[tokio::test]
async fn idempotent_queries_continue_after_a_candidate_disconnects() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let first_path = directory.path().join("first.sock");
    let second_path = directory.path().join("second.sock");
    let fallback_path = directory.path().join("fallback.sock");
    let lanyard_path = directory.path().join("lanyard.sock");
    let first_listener = UnixListener::bind(&first_path)?;
    let second_listener = UnixListener::bind(&second_path)?;
    let _fallback_listener = UnixListener::bind(&fallback_path)?;
    let first = tokio::spawn(async move {
        let (mut stream, _) = first_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, QUERY_EXTENSION);
        Ok::<(), io::Error>(())
    });
    let second = tokio::spawn(async move {
        let (mut stream, _) = second_listener.accept().await?;
        assert_eq!(read_frame(&mut stream).await?, QUERY_EXTENSION);
        write_frame(&mut stream, &[6]).await
    });
    let listener = UnixListener::bind(&lanyard_path)?;
    let proxy = tokio::spawn(serve(
        listener,
        Config::new(fallback_path).with_registered(vec![first_path, second_path]),
    ));
    let mut client = UnixStream::connect(&lanyard_path).await?;

    write_frame(&mut client, QUERY_EXTENSION).await?;
    assert_eq!(read_frame(&mut client).await?, [6]);

    timeout(Duration::from_secs(1), first).await???;
    timeout(Duration::from_secs(1), second).await???;
    proxy.abort();
    Ok(())
}

fn fake_agent(listener: UnixListener, response: Vec<u8>) -> JoinHandle<io::Result<()>> {
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        assert_eq!(
            read_frame(&mut stream).await?,
            REQUEST_IDENTITIES,
            "fake agent should receive an identity request"
        );
        write_frame(&mut stream, &response).await
    })
}

fn fake_request(
    listener: UnixListener,
    expected_request: &'static [u8],
    response: Vec<u8>,
) -> JoinHandle<io::Result<()>> {
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        assert_eq!(
            read_frame(&mut stream).await?,
            expected_request,
            "fake agent should receive the expected request"
        );
        write_frame(&mut stream, &response).await
    })
}

fn query_response(extensions: &[&[u8]]) -> Vec<u8> {
    let mut response = vec![29];
    append_string(&mut response, b"query");
    for extension in extensions {
        append_string(&mut response, extension);
    }
    response
}

fn identities_response(identities: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut response = vec![12];
    response.extend(
        u32::try_from(identities.len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    #[expect(
        clippy::pattern_type_mismatch,
        reason = "borrowed tuple iteration is idiomatic"
    )]
    for (key, comment) in identities {
        append_string(&mut response, key);
        append_string(&mut response, comment);
    }
    response
}

fn append_string(message: &mut Vec<u8>, value: &[u8]) {
    message.extend(u32::try_from(value.len()).unwrap_or(u32::MAX).to_be_bytes());
    message.extend(value);
}

async fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    timeout(Duration::from_secs(1), async {
        let length = stream.read_u32().await?;
        let mut body = vec![0; usize::try_from(length).unwrap_or(usize::MAX)];
        stream.read_exact(&mut body).await?;
        Ok(body)
    })
    .await
    .map_err(|_elapsed| io::Error::new(io::ErrorKind::TimedOut, "frame timed out"))?
}

async fn write_frame(stream: &mut UnixStream, body: &[u8]) -> io::Result<()> {
    stream
        .write_u32(u32::try_from(body.len()).unwrap_or(u32::MAX))
        .await?;
    stream.write_all(body).await
}
