//! XDG paths used by the daemon and its clients.

use std::path::{Path, PathBuf};

/// Names the stable SSH agent socket beneath an XDG runtime directory.
#[must_use]
#[inline]
pub fn agent_socket(runtime_directory: &Path) -> PathBuf {
    runtime_directory.join("lanyard-ssh-agent/agent.sock")
}

/// Names the daemon control socket beneath an XDG runtime directory.
#[must_use]
#[inline]
pub fn control_socket(runtime_directory: &Path) -> PathBuf {
    runtime_directory.join("lanyard-ssh-agent/control.sock")
}

#[cfg(test)]
mod tests {
    use super::{agent_socket, control_socket};
    use std::path::Path;

    #[test]
    fn stable_socket_names_share_a_private_runtime_directory() {
        let runtime = Path::new("/run/user/1000");

        assert_eq!(
            agent_socket(runtime),
            Path::new("/run/user/1000/lanyard-ssh-agent/agent.sock")
        );
        assert_eq!(
            control_socket(runtime),
            Path::new("/run/user/1000/lanyard-ssh-agent/control.sock")
        );
    }
}
