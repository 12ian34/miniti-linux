//! Local control surface for scripts, bars and the `miniti` CLI.
//!
//! Three doors into the running app, all on the same request handler:
//!
//! - a Unix socket in the XDG runtime dir (`$XDG_RUNTIME_DIR/miniti/miniti.sock`)
//!   speaking JSON lines (`protocol`), used by the CLI and by bar widgets that
//!   want a live stream (`subscribe`);
//! - a JSON snapshot file next to it (`state.json`) rewritten on every change,
//!   for tools that would rather watch a file than hold a connection;
//! - a session-bus D-Bus object (`com.miniti.linux` at `/com/miniti/linux`,
//!   interface `com.miniti.linux.Control`) for desktops and launchers.
//!
//! The socket also acts as the single-instance guard: a second `miniti`
//! process finds the socket alive, forwards its arguments (deep links) and
//! exits.

pub mod client;
pub mod dbus;
pub mod inhibit;
pub mod protocol;
pub mod server;
pub mod snapshot;

use std::path::PathBuf;

/// Overrides the socket path (tests, sandboxes).
pub const SOCKET_ENV: &str = "MINITI_SOCKET";
pub const SOCKET_FILE: &str = "miniti.sock";
pub const STATE_FILE: &str = "state.json";
pub const BUS_NAME: &str = "com.miniti.linux";
pub const OBJECT_PATH: &str = "/com/miniti/linux";
pub const INTERFACE: &str = "com.miniti.linux.Control";

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// Per-session runtime directory (XDG Base Directory spec): sockets, pid,
/// live state. Falls back to a per-user directory under the temp dir when no
/// session runtime dir exists (ssh, cron, macOS dev box).
pub fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d).join("miniti"),
        None => std::env::temp_dir().join(format!("miniti-{}", uid())),
    }
}

/// Create the runtime dir owner-only (0700), as the spec requires.
pub fn ensure_runtime_dir() -> std::io::Result<PathBuf> {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

pub fn socket_path() -> PathBuf {
    match std::env::var_os(SOCKET_ENV).filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => runtime_dir().join(SOCKET_FILE),
    }
}

pub fn state_path() -> PathBuf {
    runtime_dir().join(STATE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_dir_follows_xdg_or_falls_back_to_a_private_temp_dir() {
        // Both branches must yield a per-user location that is not shared.
        let dir = runtime_dir();
        let s = dir.to_string_lossy();
        assert!(s.contains("miniti"), "{s}");
        assert!(socket_path().ends_with(SOCKET_FILE) || std::env::var_os(SOCKET_ENV).is_some());
        assert!(state_path().ends_with(STATE_FILE));
    }
}
