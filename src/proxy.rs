//! Read-only SSH-agent protocol forwarding.

use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::timeout;

const REQUEST_IDENTITIES: u8 = 11;
const SIGN_REQUEST: u8 = 13;
const EXTENSION: u8 = 27;
const EXTENSION_NAME_OFFSET: usize = 5;
const FAILURE: &[u8] = &[5];
const SUCCESS: &[u8] = &[6];
const DEFAULT_MAXIMUM_MESSAGE_BYTES: usize = 256 * 1024;
const DEFAULT_MAXIMUM_CANDIDATES: usize = 32;

/// Time limits for contacting and operating an upstream agent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timeouts {
    connect: Duration,
    identities: Duration,
    sign: Duration,
}

impl Timeouts {
    /// Creates an explicit timeout policy.
    #[must_use]
    #[inline]
    pub const fn new(connect: Duration, identities: Duration, sign: Duration) -> Self {
        Self {
            connect,
            identities,
            sign,
        }
    }

    /// Returns the upstream connection time limit.
    #[must_use]
    #[inline]
    pub const fn connect(self) -> Duration {
        self.connect
    }

    /// Returns the identity and extension operation time limit.
    #[must_use]
    #[inline]
    pub const fn identities(self) -> Duration {
        self.identities
    }

    /// Returns the signing operation time limit.
    #[must_use]
    #[inline]
    pub const fn sign(self) -> Duration {
        self.sign
    }
}

impl Default for Timeouts {
    #[inline]
    fn default() -> Self {
        Self::new(
            Duration::from_millis(500),
            Duration::from_secs(2),
            Duration::from_secs(30),
        )
    }
}

/// Runtime limits and the sole upstream agent for a proxy instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    upstream: PathBuf,
    maximum_message_bytes: usize,
    timeouts: Timeouts,
}

impl Config {
    /// Creates a proxy configuration with Lanyard's documented safety bounds.
    #[must_use]
    #[inline]
    pub fn new<Upstream>(upstream: Upstream) -> Self
    where
        Upstream: Into<PathBuf>,
    {
        Self {
            upstream: upstream.into(),
            maximum_message_bytes: DEFAULT_MAXIMUM_MESSAGE_BYTES,
            timeouts: Timeouts::default(),
        }
    }

    /// Returns the maximum accepted SSH-agent message body size.
    #[must_use]
    #[inline]
    pub const fn maximum_message_bytes(&self) -> usize {
        self.maximum_message_bytes
    }

    /// Returns the project-wide cap on candidate upstream agents.
    #[must_use]
    #[inline]
    pub const fn maximum_candidates(&self) -> usize {
        DEFAULT_MAXIMUM_CANDIDATES
    }

    /// Overrides operation time limits.
    #[must_use]
    #[inline]
    pub const fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    /// Returns the configured operation time limits.
    #[must_use]
    #[inline]
    pub const fn timeouts(&self) -> Timeouts {
        self.timeouts
    }
}

/// Serves proxy connections until accepting a downstream connection fails.
///
/// # Errors
///
/// Returns an error when the listener can no longer accept connections.
#[inline]
pub async fn serve(listener: UnixListener, config: Config) -> io::Result<()> {
    loop {
        let (downstream, _) = listener.accept().await?;
        let connection_config = config.clone();
        tokio::spawn(async move {
            let _connection_result = handle_connection(downstream, connection_config).await;
        });
    }
}

async fn handle_connection(mut downstream: UnixStream, config: Config) -> io::Result<()> {
    let mut upstream = None;
    let mut session_bindings = Vec::new();
    loop {
        let request = read_frame(&mut downstream, config.maximum_message_bytes).await?;
        let Some(message_type) = request.first().copied() else {
            write_frame(&mut downstream, FAILURE).await?;
            continue;
        };

        let Some(response_timeout) = response_timeout(message_type, &request, config.timeouts)
        else {
            write_frame(&mut downstream, FAILURE).await?;
            continue;
        };

        if is_session_bind(&request) && session_bindings.len() >= DEFAULT_MAXIMUM_CANDIDATES {
            write_frame(&mut downstream, FAILURE).await?;
            continue;
        }

        if upstream.is_none() {
            if let Ok(stream) = connect_bound(&config, &session_bindings).await {
                upstream = Some(stream);
            } else {
                write_frame(&mut downstream, FAILURE).await?;
                continue;
            }
        }

        let mut result = forward(
            upstream.as_mut().ok_or_else(|| {
                io::Error::other("upstream connection disappeared before forwarding")
            })?,
            &request,
            response_timeout,
            config.maximum_message_bytes,
        )
        .await;

        if result.is_err() && retry_after_disconnect(message_type) {
            upstream = None;
            if let Ok(stream) = connect_bound(&config, &session_bindings).await {
                upstream = Some(stream);
                result = forward(
                    upstream.as_mut().ok_or_else(|| {
                        io::Error::other("reconnected upstream disappeared before forwarding")
                    })?,
                    &request,
                    response_timeout,
                    config.maximum_message_bytes,
                )
                .await;
            }
        }

        if let Ok(response) = result {
            if should_remember_binding(&request, &response) {
                session_bindings.push(request.clone());
            }
            write_frame(&mut downstream, &response).await?;
        } else {
            upstream = None;
            write_frame(&mut downstream, FAILURE).await?;
        }
    }
}

async fn connect_bound(config: &Config, session_bindings: &[Vec<u8>]) -> io::Result<UnixStream> {
    let mut stream = connect(&config.upstream, config.timeouts.connect).await?;
    for binding in session_bindings {
        let response = forward(
            &mut stream,
            binding,
            config.timeouts.identities,
            config.maximum_message_bytes,
        )
        .await?;
        if response != SUCCESS {
            return Err(io::Error::other(
                "upstream rejected replayed session binding",
            ));
        }
    }
    Ok(stream)
}

async fn connect(path: &Path, connect_timeout: Duration) -> io::Result<UnixStream> {
    timeout(connect_timeout, UnixStream::connect(path))
        .await
        .map_err(|_elapsed| io::Error::new(ErrorKind::TimedOut, "upstream connect timed out"))?
}

async fn forward(
    upstream: &mut UnixStream,
    request: &[u8],
    response_timeout: Duration,
    maximum_message_bytes: usize,
) -> io::Result<Vec<u8>> {
    timeout(response_timeout, async {
        write_frame(upstream, request).await?;
        read_frame(upstream, maximum_message_bytes).await
    })
    .await
    .map_err(|_elapsed| io::Error::new(ErrorKind::TimedOut, "upstream operation timed out"))?
}

const fn retry_after_disconnect(message_type: u8) -> bool {
    message_type == REQUEST_IDENTITIES || message_type == EXTENSION
}

fn response_timeout(message_type: u8, request: &[u8], timeouts: Timeouts) -> Option<Duration> {
    match message_type {
        REQUEST_IDENTITIES => Some(timeouts.identities),
        SIGN_REQUEST => Some(timeouts.sign),
        EXTENSION if is_supported_extension(request) => Some(timeouts.identities),
        _ => None,
    }
}

fn is_supported_extension(request: &[u8]) -> bool {
    extension_name(request)
        .is_some_and(|name| name == b"query" || name == b"session-bind@openssh.com")
}

fn is_session_bind(request: &[u8]) -> bool {
    extension_name(request) == Some(b"session-bind@openssh.com")
}

fn should_remember_binding(request: &[u8], response: &[u8]) -> bool {
    is_session_bind(request) && response == SUCCESS
}

fn extension_name(request: &[u8]) -> Option<&[u8]> {
    let length_bytes = request.get(1..5)?;
    let encoded_length = <[u8; 4]>::try_from(length_bytes).ok()?;
    let name_length = usize::try_from(u32::from_be_bytes(encoded_length)).unwrap_or(usize::MAX);
    request.get(EXTENSION_NAME_OFFSET..EXTENSION_NAME_OFFSET.saturating_add(name_length))
}

#[cfg_attr(test, mutants::skip)]
async fn read_frame(
    stream: &mut (impl AsyncRead + Unpin),
    maximum_message_bytes: usize,
) -> io::Result<Vec<u8>> {
    let encoded_length = stream.read_u32().await?;
    let length = validate_message_length(encoded_length, maximum_message_bytes)?;
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

fn validate_message_length(encoded_length: u32, maximum_message_bytes: usize) -> io::Result<usize> {
    let length = usize::try_from(encoded_length).map_err(|_conversion| {
        io::Error::new(ErrorKind::InvalidData, "message length is unsupported")
    })?;
    if length == 0 || length > maximum_message_bytes {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "SSH agent message exceeds protocol bounds",
        ));
    }
    Ok(length)
}

#[cfg_attr(test, mutants::skip)]
async fn write_frame(stream: &mut (impl AsyncWrite + Unpin), body: &[u8]) -> io::Result<()> {
    let length = u32::try_from(body.len())
        .map_err(|_conversion| io::Error::new(ErrorKind::InvalidInput, "response is too large"))?;
    stream.write_u32(length).await?;
    stream.write_all(body).await
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use super::{
        EXTENSION, FAILURE, REQUEST_IDENTITIES, SIGN_REQUEST, SUCCESS, Timeouts, response_timeout,
        retry_after_disconnect, should_remember_binding, validate_message_length,
    };

    const QUERY: &[u8] = &[EXTENSION, 0, 0, 0, 5, b'q', b'u', b'e', b'r', b'y'];
    const SESSION_BIND: &[u8] = &[
        EXTENSION, 0, 0, 0, 24, b's', b'e', b's', b's', b'i', b'o', b'n', b'-', b'b', b'i', b'n',
        b'd', b'@', b'o', b'p', b'e', b'n', b's', b's', b'h', b'.', b'c', b'o', b'm',
    ];

    #[test]
    fn validates_message_length_boundaries() {
        assert_eq!(validate_message_length(1, 16).ok(), Some(1));
        assert_eq!(validate_message_length(16, 16).ok(), Some(16));
        assert_eq!(
            validate_message_length(0, 16)
                .err()
                .map(|error| error.kind()),
            Some(ErrorKind::InvalidData)
        );
        assert_eq!(
            validate_message_length(17, 16)
                .err()
                .map(|error| error.kind()),
            Some(ErrorKind::InvalidData)
        );
    }

    #[test]
    fn classifies_allowed_operations_and_timeouts() {
        let timeouts = Timeouts::default();

        assert_eq!(
            response_timeout(REQUEST_IDENTITIES, &[REQUEST_IDENTITIES], timeouts),
            Some(timeouts.identities)
        );
        assert_eq!(
            response_timeout(SIGN_REQUEST, &[SIGN_REQUEST], timeouts),
            Some(timeouts.sign)
        );
        assert_eq!(
            response_timeout(EXTENSION, QUERY, timeouts),
            Some(timeouts.identities)
        );
        assert_eq!(
            response_timeout(EXTENSION, SESSION_BIND, timeouts),
            Some(timeouts.identities)
        );
        assert_eq!(response_timeout(EXTENSION, &[EXTENSION], timeouts), None);
        assert_eq!(response_timeout(17, &[17], timeouts), None);
    }

    #[test]
    fn retries_only_idempotent_operations() {
        assert!(retry_after_disconnect(REQUEST_IDENTITIES));
        assert!(retry_after_disconnect(EXTENSION));
        assert!(!retry_after_disconnect(SIGN_REQUEST));
    }

    #[test]
    fn remembers_only_acknowledged_session_bindings() {
        assert!(should_remember_binding(SESSION_BIND, SUCCESS));
        assert!(!should_remember_binding(SESSION_BIND, FAILURE));
        assert!(!should_remember_binding(QUERY, SUCCESS));
    }
}
