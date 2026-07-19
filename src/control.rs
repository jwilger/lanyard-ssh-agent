//! Local JSON-lines control protocol for a running Lanyard daemon.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use tokio::time::timeout;

use crate::backend::{SharedRegistry, Source};
use crate::proxy::discover_agent_sockets;

const MAXIMUM_CONTROL_MESSAGE_BYTES: u64 = 0x0004_0000;
const REACHABILITY_TIMEOUT: Duration = Duration::from_millis(500);

/// A request accepted by the daemon control socket.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
#[expect(
    clippy::exhaustive_enums,
    reason = "the versioned wire protocol is closed"
)]
pub enum Request {
    /// Add or refresh an explicit upstream.
    Register {
        /// Upstream Unix socket path.
        #[serde(with = "wire_path")]
        path: PathBuf,
    },
    /// Remove an explicit upstream.
    Unregister {
        /// Upstream Unix socket path.
        #[serde(with = "wire_path")]
        path: PathBuf,
    },
    /// Inspect the current ordered candidate set.
    Status,
}

/// One candidate in a status response.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "the versioned wire protocol is closed"
)]
pub struct CandidateStatus {
    /// Socket path.
    #[serde(with = "wire_path")]
    pub path: PathBuf,
    /// Candidate origin.
    pub source: Source,
    /// Whether a connection succeeds at status time.
    pub reachable: bool,
}

/// A response returned by the daemon control socket.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
#[expect(
    clippy::exhaustive_enums,
    reason = "the versioned wire protocol is closed"
)]
pub enum Response {
    /// Mutation acknowledgement.
    Updated {
        /// Whether daemon state changed.
        changed: bool,
    },
    /// Ordered upstream state.
    Status {
        /// Candidates in routing priority order.
        candidates: Vec<CandidateStatus>,
    },
    /// Invalid request or daemon error.
    Error {
        /// Human-readable failure description.
        message: String,
    },
}

/// Serves control connections until accepting a connection fails.
///
/// # Errors
///
/// Returns an error when the listener can no longer accept connections.
#[inline]
pub async fn serve(
    listener: UnixListener,
    registry: SharedRegistry,
    discovery_roots: Vec<PathBuf>,
) -> io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let connection_registry = registry.clone();
        let connection_roots = discovery_roots.clone();
        tokio::spawn(async move {
            let _result = handle(stream, connection_registry, connection_roots).await;
        });
    }
}

async fn handle(
    stream: UnixStream,
    registry: SharedRegistry,
    discovery_roots: Vec<PathBuf>,
) -> io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut line = Vec::new();
    BufReader::new(read)
        .take(MAXIMUM_CONTROL_MESSAGE_BYTES.saturating_add(1))
        .read_until(b'\n', &mut line)
        .await?;
    let response = if u64::try_from(line.len()).unwrap_or(u64::MAX) > MAXIMUM_CONTROL_MESSAGE_BYTES
    {
        Response::Error {
            message: "control request exceeds protocol bounds".to_owned(),
        }
    } else {
        match line.strip_suffix(b"\n") {
            Some(request) => dispatch(request, registry, discovery_roots).await,
            None => Response::Error {
                message: "control request must end with a newline".to_owned(),
            },
        }
    };
    let mut encoded = serde_json::to_vec(&response).map_err(io::Error::other)?;
    encoded.push(b'\n');
    write.write_all(&encoded).await
}

async fn dispatch(
    request: &[u8],
    registry: SharedRegistry,
    discovery_roots: Vec<PathBuf>,
) -> Response {
    match serde_json::from_slice::<Request>(request) {
        Ok(Request::Register { path }) => {
            registry.register(path);
            Response::Updated { changed: true }
        }
        Ok(Request::Unregister { path }) => Response::Updated {
            changed: registry.unregister(&path),
        },
        Ok(Request::Status) => {
            let discovered = discover_agent_sockets(&discovery_roots).await;
            let backends = registry.candidates(discovered);
            let mut probes = JoinSet::new();
            for (index, backend) in backends.into_iter().enumerate() {
                probes.spawn(async move {
                    let reachable =
                        timeout(REACHABILITY_TIMEOUT, UnixStream::connect(backend.path()))
                            .await
                            .is_ok_and(|result| result.is_ok());
                    (
                        index,
                        CandidateStatus {
                            path: backend.path().to_path_buf(),
                            source: backend.source(),
                            reachable,
                        },
                    )
                });
            }
            let mut candidates = vec![None; probes.len()];
            while let Some(Ok((index, status))) = probes.join_next().await {
                if let Some(slot) = candidates.get_mut(index) {
                    *slot = Some(status);
                }
            }
            Response::Status {
                candidates: candidates.into_iter().flatten().collect(),
            }
        }
        Err(error) => Response::Error {
            message: format!("invalid request: {error}"),
        },
    }
}

mod wire_path {
    use super::{OsString, PathBuf};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
    use std::path::Path;

    #[derive(Serialize)]
    #[serde(untagged)]
    enum Borrowed<'path> {
        Utf8(&'path str),
        UnixBytes { unix_bytes: &'path [u8] },
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Owned {
        Utf8(String),
        UnixBytes { unix_bytes: Vec<u8> },
    }

    pub fn serialize<SerializerType>(
        path: &Path,
        serializer: SerializerType,
    ) -> Result<SerializerType::Ok, SerializerType::Error>
    where
        SerializerType: Serializer,
    {
        match path.to_str() {
            Some(utf8_path) => Borrowed::Utf8(utf8_path).serialize(serializer),
            None => Borrowed::UnixBytes {
                unix_bytes: path.as_os_str().as_bytes(),
            }
            .serialize(serializer),
        }
    }

    pub fn deserialize<'de, DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<PathBuf, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        Ok(match Owned::deserialize(deserializer)? {
            Owned::Utf8(path) => PathBuf::from(path),
            Owned::UnixBytes { unix_bytes } => PathBuf::from(OsString::from_vec(unix_bytes)),
        })
    }
}
