//! Read-only SSH-agent protocol forwarding.

use std::collections::HashSet;
use std::io::{self, ErrorKind};
use std::os::unix::fs::{FileTypeExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustix::process::geteuid;
use tokio::fs::{read_dir, symlink_metadata};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::backend::{Backend, MAXIMUM_CANDIDATES, Registry, SharedRegistry};

const REQUEST_IDENTITIES: u8 = 11;
const SIGN_REQUEST: u8 = 13;
const SIGN_RESPONSE: u8 = 14;
const EXTENSION: u8 = 27;
const EXTENSION_FAILURE: &[u8] = &[28];
const EXTENSION_RESPONSE: u8 = 29;
const EXTENSION_NAME_OFFSET: usize = 5;
const FAILURE: &[u8] = &[5];
const SUCCESS: &[u8] = &[6];
const DEFAULT_MAXIMUM_MESSAGE_BYTES: usize = 256 * 1024;

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

/// Runtime limits and upstream-agent sources for a proxy instance.
#[derive(Clone, Debug)]
pub struct Config {
    registry: SharedRegistry,
    discovery_roots: Vec<PathBuf>,
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
            registry: SharedRegistry::new(Registry::new(upstream.into())),
            discovery_roots: vec![PathBuf::from("/tmp")],
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
        MAXIMUM_CANDIDATES
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

    /// Adds explicitly registered upstreams in the supplied order.
    #[must_use]
    #[inline]
    pub fn with_registered(self, registered: Vec<PathBuf>) -> Self {
        for path in registered.into_iter().rev() {
            self.registry.register(path);
        }
        self
    }

    /// Overrides roots searched for OpenSSH-style forwarded agent sockets.
    #[must_use]
    #[inline]
    pub fn with_discovery_roots(mut self, discovery_roots: Vec<PathBuf>) -> Self {
        self.discovery_roots = discovery_roots;
        self
    }

    /// Returns the shared runtime registry.
    #[must_use]
    #[inline]
    pub fn registry(&self) -> SharedRegistry {
        self.registry.clone()
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

        if is_session_bind(&request) && session_bindings.len() >= MAXIMUM_CANDIDATES {
            write_frame(&mut downstream, FAILURE).await?;
            continue;
        }

        if message_type == REQUEST_IDENTITIES {
            match aggregate_identities(&config, &session_bindings).await {
                Ok(response) => write_frame(&mut downstream, &response).await?,
                Err(_) => write_frame(&mut downstream, FAILURE).await?,
            }
            continue;
        }

        match connect_and_forward_candidate(&config, &session_bindings, &request, response_timeout)
            .await
        {
            Ok(response) => {
                if should_remember_binding(&request, &response) {
                    session_bindings.push(request.clone());
                }
                write_frame(&mut downstream, &response).await?;
            }
            Err(_) => write_frame(&mut downstream, FAILURE).await?,
        }
    }
}

async fn connect_and_forward_candidate(
    config: &Config,
    session_bindings: &[Vec<u8>],
    request: &[u8],
    response_timeout: Duration,
) -> io::Result<Vec<u8>> {
    let discovered = discover_agent_sockets(&config.discovery_roots).await;
    for backend in config.registry.candidates(discovered) {
        let Ok(mut stream) = connect_bound(config, backend.path(), session_bindings).await else {
            continue;
        };
        match forward(
            &mut stream,
            request,
            response_timeout,
            config.maximum_message_bytes,
        )
        .await
        {
            Ok(response) if response_accepts_request(request, &response) => return Ok(response),
            Ok(response)
                if is_session_bind(request)
                    && response != FAILURE
                    && response != EXTENSION_FAILURE =>
            {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "upstream returned an ambiguous session-binding response",
                ));
            }
            Err(error)
                if request.first().is_some_and(|message_type| {
                    !retry_after_disconnect(*message_type, request)
                }) =>
            {
                return Err(error);
            }
            Ok(_) | Err(_) => {}
        }
    }
    Err(io::Error::other("no upstream agent accepted the request"))
}

async fn connect_bound(
    config: &Config,
    path: &Path,
    session_bindings: &[Vec<u8>],
) -> io::Result<UnixStream> {
    let mut stream = connect(path, config.timeouts.connect).await?;
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

async fn aggregate_identities(
    config: &Config,
    session_bindings: &[Vec<u8>],
) -> io::Result<Vec<u8>> {
    let discovered = discover_agent_sockets(&config.discovery_roots).await;
    let candidates = config.registry.candidates(discovered);
    let mut tasks = JoinSet::new();
    for (index, backend) in candidates.into_iter().enumerate() {
        let task_config = config.clone();
        let task_bindings = session_bindings.to_vec();
        tasks.spawn(async move {
            let response = request_identities(&task_config, &backend, &task_bindings).await;
            (index, response)
        });
    }

    let mut responses = vec![None; tasks.len()];
    while let Some(task_result) = tasks.join_next().await {
        if let Ok((index, Ok(response))) = task_result
            && let Some(slot) = responses.get_mut(index)
        {
            *slot = Some(response);
        }
    }
    merge_identity_responses(
        responses.into_iter().flatten(),
        config.maximum_message_bytes,
    )
}

async fn request_identities(
    config: &Config,
    backend: &Backend,
    session_bindings: &[Vec<u8>],
) -> io::Result<Vec<u8>> {
    let mut stream = connect_bound(config, backend.path(), session_bindings).await?;
    forward(
        &mut stream,
        &[REQUEST_IDENTITIES],
        config.timeouts.identities,
        config.maximum_message_bytes,
    )
    .await
}

#[expect(
    clippy::pub_with_shorthand,
    reason = "the inverse restriction lint is also enabled by the strict profile"
)]
pub(crate) async fn discover_agent_sockets(roots: &[PathBuf]) -> Vec<PathBuf> {
    let expected_uid = geteuid().as_raw();
    let mut sockets = Vec::new();
    for root in roots {
        let Ok(mut entries) = read_dir(root).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            if !name.to_string_lossy().starts_with("ssh-") {
                continue;
            }
            let Ok(mut agent_entries) = read_dir(entry.path()).await else {
                continue;
            };
            while let Ok(Some(agent_entry)) = agent_entries.next_entry().await {
                if !agent_entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("agent.")
                {
                    continue;
                }
                let path = agent_entry.path();
                let Ok(metadata) = symlink_metadata(&path).await else {
                    continue;
                };
                if metadata.file_type().is_socket() && metadata.uid() == expected_uid {
                    sockets.push(path);
                }
            }
        }
    }
    sockets.sort();
    sockets
}

fn merge_identity_responses(
    responses: impl IntoIterator<Item = Vec<u8>>,
    maximum_message_bytes: usize,
) -> io::Result<Vec<u8>> {
    let mut seen = HashSet::new();
    let mut identities = Vec::new();
    let mut received_valid_response = false;
    let mut encoded_length: usize = 5;
    for response in responses {
        let Ok(parsed) = parse_identities(&response) else {
            continue;
        };
        received_valid_response = true;
        for identity in parsed {
            if seen.insert(identity.0.clone()) {
                let identity_prefix_bytes: usize = 8;
                let identity_length = identity_prefix_bytes
                    .checked_add(identity.0.len())
                    .and_then(|length| length.checked_add(identity.1.len()))
                    .ok_or_else(|| io::Error::other("identity response length overflowed"))?;
                encoded_length = encoded_length
                    .checked_add(identity_length)
                    .ok_or_else(|| io::Error::other("identity union length overflowed"))?;
                if encoded_length > maximum_message_bytes {
                    return Err(io::Error::new(
                        ErrorKind::InvalidData,
                        "aggregated identity response exceeds protocol bounds",
                    ));
                }
                identities.push(identity);
            }
        }
    }
    if !received_valid_response {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "no upstream returned a valid identity response",
        ));
    }
    encode_identities(&identities)
}

fn response_accepts_request(request: &[u8], response: &[u8]) -> bool {
    match request.first().copied() {
        Some(SIGN_REQUEST) => valid_sign_response(response),
        Some(EXTENSION) if is_session_bind(request) => response == SUCCESS,
        Some(EXTENSION) if extension_name(request) == Some(b"query") => {
            valid_query_response(response)
        }
        _ => false,
    }
}

fn valid_query_response(response: &[u8]) -> bool {
    if response == SUCCESS {
        return true;
    }
    let Some((&message_type, payload)) = response.split_first() else {
        return false;
    };
    if message_type != EXTENSION_RESPONSE {
        return false;
    }
    let mut cursor = Cursor::new(payload);
    if cursor.read_string().ok().as_deref() != Some(b"query") {
        return false;
    }
    while !cursor.remaining().is_empty() {
        if cursor.read_string().is_err() {
            return false;
        }
    }
    true
}

fn valid_sign_response(response: &[u8]) -> bool {
    let Some((&message_type, payload)) = response.split_first() else {
        return false;
    };
    if message_type != SIGN_RESPONSE {
        return false;
    }
    let mut cursor = Cursor::new(payload);
    cursor.read_string().is_ok() && cursor.remaining().is_empty()
}

fn parse_identities(response: &[u8]) -> io::Result<Vec<(Vec<u8>, Vec<u8>)>> {
    let Some((&message_type, payload)) = response.split_first() else {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "empty identity response",
        ));
    };
    if message_type != 12 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "unexpected identity response",
        ));
    }
    let mut cursor = Cursor::new(payload);
    let count = usize::try_from(cursor.read_u32()?).map_err(|_conversion| {
        io::Error::new(ErrorKind::InvalidData, "identity count is invalid")
    })?;
    if count > response.len().saturating_div(8) {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "identity count exceeds message",
        ));
    }
    let mut identities = Vec::with_capacity(count);
    for _index in 0..count {
        identities.push((cursor.read_string()?, cursor.read_string()?));
    }
    if !cursor.remaining().is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "identity response has trailing data",
        ));
    }
    Ok(identities)
}

fn encode_identities(identities: &[(Vec<u8>, Vec<u8>)]) -> io::Result<Vec<u8>> {
    let mut response = vec![12];
    append_u32(&mut response, identities.len())?;
    #[expect(
        clippy::pattern_type_mismatch,
        reason = "the idiomatic borrowed tuple pattern is clearest here"
    )]
    for (key, comment) in identities {
        append_string(&mut response, key)?;
        append_string(&mut response, comment)?;
    }
    Ok(response)
}

fn append_string(message: &mut Vec<u8>, value: &[u8]) -> io::Result<()> {
    append_u32(message, value.len())?;
    message.extend_from_slice(value);
    Ok(())
}

fn append_u32(message: &mut Vec<u8>, value: usize) -> io::Result<()> {
    let encoded = u32::try_from(value)
        .map_err(|_conversion| io::Error::new(ErrorKind::InvalidData, "value exceeds protocol"))?;
    message.extend_from_slice(&encoded.to_be_bytes());
    Ok(())
}

struct Cursor<'message> {
    remaining: &'message [u8],
}

impl<'message> Cursor<'message> {
    const fn new(message: &'message [u8]) -> Self {
        Self { remaining: message }
    }

    fn read_u32(&mut self) -> io::Result<u32> {
        let bytes = self.take(4)?;
        let encoded = <[u8; 4]>::try_from(bytes)
            .map_err(|_conversion| io::Error::new(ErrorKind::InvalidData, "invalid u32"))?;
        Ok(u32::from_be_bytes(encoded))
    }

    fn read_string(&mut self) -> io::Result<Vec<u8>> {
        let length = usize::try_from(self.read_u32()?).map_err(|_conversion| {
            io::Error::new(ErrorKind::InvalidData, "invalid string length")
        })?;
        Ok(self.take(length)?.to_vec())
    }

    fn take(&mut self, length: usize) -> io::Result<&'message [u8]> {
        let Some((value, remaining)) = self.remaining.split_at_checked(length) else {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                "truncated SSH agent message",
            ));
        };
        self.remaining = remaining;
        Ok(value)
    }

    const fn remaining(&self) -> &'message [u8] {
        self.remaining
    }
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

fn retry_after_disconnect(message_type: u8, request: &[u8]) -> bool {
    message_type == REQUEST_IDENTITIES
        || message_type == SIGN_REQUEST
        || (message_type == EXTENSION && !is_session_bind(request))
}

fn response_timeout(message_type: u8, request: &[u8], timeouts: Timeouts) -> Option<Duration> {
    match message_type {
        REQUEST_IDENTITIES if request == [REQUEST_IDENTITIES] => Some(timeouts.identities),
        SIGN_REQUEST if valid_sign_request(request) => Some(timeouts.sign),
        EXTENSION if valid_supported_extension_request(request) => Some(timeouts.identities),
        _ => None,
    }
}

fn valid_sign_request(request: &[u8]) -> bool {
    let Some((&message_type, payload)) = request.split_first() else {
        return false;
    };
    let mut cursor = Cursor::new(payload);
    message_type == SIGN_REQUEST
        && cursor.read_string().is_ok()
        && cursor.read_string().is_ok()
        && cursor.read_u32().is_ok()
        && cursor.remaining().is_empty()
}

fn valid_supported_extension_request(request: &[u8]) -> bool {
    let Some((&message_type, payload)) = request.split_first() else {
        return false;
    };
    if message_type != EXTENSION {
        return false;
    }
    let mut cursor = Cursor::new(payload);
    let Ok(name) = cursor.read_string() else {
        return false;
    };
    if name == b"query" {
        return cursor.remaining().is_empty();
    }
    name == b"session-bind@openssh.com"
        && cursor.read_string().is_ok()
        && cursor.read_string().is_ok()
        && cursor.read_string().is_ok()
        && cursor.take(1).is_ok()
        && cursor.remaining().is_empty()
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
        EXTENSION, FAILURE, REQUEST_IDENTITIES, SIGN_REQUEST, SUCCESS, Timeouts,
        merge_identity_responses, parse_identities, response_accepts_request, response_timeout,
        retry_after_disconnect, should_remember_binding, validate_message_length,
    };

    const QUERY: &[u8] = &[EXTENSION, 0, 0, 0, 5, b'q', b'u', b'e', b'r', b'y'];
    const SIGN: &[u8] = &[SIGN_REQUEST, 0, 0, 0, 1, b'k', 0, 0, 0, 0, 0, 0, 0, 0];
    const SESSION_BIND: &[u8] = &[
        EXTENSION, 0, 0, 0, 24, b's', b'e', b's', b's', b'i', b'o', b'n', b'-', b'b', b'i', b'n',
        b'd', b'@', b'o', b'p', b'e', b'n', b's', b's', b'h', b'.', b'c', b'o', b'm', 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0,
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
    fn accepts_an_identity_count_at_the_conservative_size_boundary() {
        let response = [12, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            parse_identities(&response).ok(),
            Some(vec![(Vec::new(), Vec::new())])
        );
    }

    #[test]
    fn accepts_an_aggregated_identity_response_at_the_exact_message_limit() {
        let response = vec![12, 0, 0, 0, 1, 0, 0, 0, 1, b'k', 0, 0, 0, 0];

        assert_eq!(
            merge_identity_responses([response], 14).ok(),
            Some(vec![12, 0, 0, 0, 1, 0, 0, 0, 1, b'k', 0, 0, 0, 0])
        );
    }

    #[test]
    fn validates_success_responses_for_each_forwarded_operation() {
        let sign_response = [14, 0, 0, 0, 1, b's'];
        let sign_response_with_trailing_data = [14, 0, 0, 0, 1, b's', b'x'];
        let success_with_payload = [6, b'x'];
        let query_response = [29, 0, 0, 0, 5, b'q', b'u', b'e', b'r', b'y'];

        assert!(response_accepts_request(&[SIGN_REQUEST], &sign_response));
        assert!(!response_accepts_request(
            &[SIGN_REQUEST],
            &sign_response_with_trailing_data
        ));
        assert!(response_accepts_request(QUERY, &query_response));
        assert!(!response_accepts_request(QUERY, &success_with_payload));
        assert!(!response_accepts_request(
            SESSION_BIND,
            &success_with_payload
        ));
        assert!(!response_accepts_request(
            &[
                EXTENSION, 0, 0, 0, 7, b'u', b'n', b'k', b'n', b'o', b'w', b'n'
            ],
            &query_response
        ));
    }

    #[test]
    fn classifies_allowed_operations_and_timeouts() {
        let timeouts = Timeouts::default();

        assert_eq!(
            response_timeout(REQUEST_IDENTITIES, &[REQUEST_IDENTITIES], timeouts),
            Some(timeouts.identities)
        );
        assert_eq!(
            response_timeout(SIGN_REQUEST, SIGN, timeouts),
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
        for malformed_sign_request in [
            vec![SIGN_REQUEST],
            vec![SIGN_REQUEST, 0, 0, 0, 1, b'k'],
            vec![SIGN_REQUEST, 0, 0, 0, 1, b'k', 0, 0, 0, 0],
            vec![SIGN_REQUEST, 0, 0, 0, 1, b'k', 0, 0, 0, 0, 0, 0, 0, 0, b'x'],
        ] {
            assert_eq!(
                response_timeout(SIGN_REQUEST, &malformed_sign_request, timeouts),
                None
            );
        }
        let truncate_binding = |removed_bytes: usize| {
            SESSION_BIND
                .iter()
                .copied()
                .take(SESSION_BIND.len().saturating_sub(removed_bytes))
                .collect::<Vec<_>>()
        };
        for malformed_session_binding in [
            truncate_binding(13),
            truncate_binding(9),
            truncate_binding(5),
            truncate_binding(1),
            [SESSION_BIND, b"x"].concat(),
        ] {
            assert_eq!(
                response_timeout(EXTENSION, &malformed_session_binding, timeouts),
                None
            );
        }
        assert_eq!(
            response_timeout(EXTENSION, &[QUERY, b"x"].concat(), timeouts),
            None
        );
        assert_eq!(
            response_timeout(REQUEST_IDENTITIES, &[REQUEST_IDENTITIES, 0], timeouts),
            None
        );
        assert_eq!(response_timeout(17, &[17], timeouts), None);
    }

    #[test]
    fn retries_only_idempotent_operations() {
        assert!(retry_after_disconnect(
            REQUEST_IDENTITIES,
            &[REQUEST_IDENTITIES]
        ));
        assert!(retry_after_disconnect(EXTENSION, QUERY));
        assert!(!retry_after_disconnect(EXTENSION, SESSION_BIND));
        assert!(retry_after_disconnect(SIGN_REQUEST, &[SIGN_REQUEST]));
    }

    #[test]
    fn remembers_only_acknowledged_session_bindings() {
        assert!(should_remember_binding(SESSION_BIND, SUCCESS));
        assert!(!should_remember_binding(SESSION_BIND, FAILURE));
        assert!(!should_remember_binding(QUERY, SUCCESS));
    }
}
