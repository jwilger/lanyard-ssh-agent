//! Upstream agent registry and deterministic candidate ordering.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{Deserialize, Serialize};

/// Maximum number of upstream candidates considered for one operation.
pub const MAXIMUM_CANDIDATES: usize = 32;

/// How an upstream agent entered the candidate set.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[expect(
    clippy::exhaustive_enums,
    reason = "wire sources are intentionally closed"
)]
pub enum Source {
    /// Explicitly registered at runtime.
    Registered,
    /// Found in an OpenSSH-style runtime directory.
    Discovered,
    /// Statically configured local fallback, normally 1Password.
    Fallback,
}

/// One ordered upstream SSH agent candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backend {
    path: PathBuf,
    source: Source,
}

impl Backend {
    const fn new(path: PathBuf, source: Source) -> Self {
        Self { path, source }
    }

    /// Returns the upstream Unix socket path.
    #[must_use]
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns how this backend entered the candidate set.
    #[must_use]
    #[inline]
    pub const fn source(&self) -> Source {
        self.source
    }
}

/// Mutable registration state with a permanent fallback backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Registry {
    fallback: PathBuf,
    registered: Vec<PathBuf>,
}

impl Registry {
    /// Creates an empty registry with a permanent fallback.
    #[must_use]
    #[inline]
    pub const fn new(fallback: PathBuf) -> Self {
        Self {
            fallback,
            registered: Vec::new(),
        }
    }

    /// Registers a path as the most-recent explicit upstream.
    #[inline]
    pub fn register(&mut self, path: PathBuf) {
        self.registered.retain(|candidate| candidate != &path);
        self.registered.insert(0, path);
    }

    /// Removes an explicitly registered path.
    #[inline]
    pub fn unregister(&mut self, path: &Path) -> bool {
        let previous_length = self.registered.len();
        self.registered.retain(|candidate| candidate != path);
        self.registered.len() != previous_length
    }

    /// Produces registered, discovered, then fallback candidates with path deduplication.
    #[must_use]
    #[inline]
    pub fn candidates(&self, discovered: Vec<PathBuf>) -> Vec<Backend> {
        let forwarded_limit = MAXIMUM_CANDIDATES.saturating_sub(1);
        let mut seen = HashSet::new();
        seen.insert(self.fallback.clone());
        let mut candidates = Vec::with_capacity(MAXIMUM_CANDIDATES);
        for (path, source) in self
            .registered
            .iter()
            .cloned()
            .map(|path| (path, Source::Registered))
            .chain(
                discovered
                    .into_iter()
                    .map(|path| (path, Source::Discovered)),
            )
        {
            if candidates.len() >= forwarded_limit {
                break;
            }
            if seen.insert(path.clone()) {
                candidates.push(Backend::new(path, source));
            }
        }
        candidates.push(Backend::new(self.fallback.clone(), Source::Fallback));
        candidates
    }
}

/// Thread-safe handle shared by agent and control listeners.
#[derive(Clone, Debug)]
pub struct SharedRegistry(Arc<RwLock<Registry>>);

impl SharedRegistry {
    /// Wraps registry state for short synchronized operations.
    #[must_use]
    #[inline]
    pub fn new(registry: Registry) -> Self {
        Self(Arc::new(RwLock::new(registry)))
    }

    /// Registers a most-recent upstream.
    #[inline]
    pub fn register(&self, path: PathBuf) {
        write_lock(&self.0).register(path);
    }

    /// Unregisters an explicit upstream.
    #[must_use]
    #[inline]
    pub fn unregister(&self, path: &Path) -> bool {
        write_lock(&self.0).unregister(path)
    }

    /// Returns an ordered snapshot using the supplied discovered paths.
    #[must_use]
    #[inline]
    pub fn candidates(&self, discovered: Vec<PathBuf>) -> Vec<Backend> {
        read_lock(&self.0).candidates(discovered)
    }
}

fn read_lock(lock: &RwLock<Registry>) -> RwLockReadGuard<'_, Registry> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write_lock(lock: &RwLock<Registry>) -> RwLockWriteGuard<'_, Registry> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}
