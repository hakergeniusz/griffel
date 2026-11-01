//! Invoking the external document converters (pandoc, xelatex, typst).
//!
//! The converters are always external processes — never bundled or linked.
//! Native installs run them directly. Inside a Flatpak sandbox they live on
//! the host, so the invocation is re-wrapped through `flatpak-spawn --host`
//! (needs the `--talk-name=org.freedesktop.Flatpak` permission); the wrapper
//! is `sh -c 'cd <dir> && exec <name> "$@"'` so the working directory and
//! argument passing behave exactly like a direct spawn. File arguments must
//! then be host-visible paths: use [`temp_dir`], not `std::env::temp_dir()`
//! (the sandbox `/tmp` does not exist on the host).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Whether the app runs inside a Flatpak sandbox.
pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Directory for converter scratch files. Inside Flatpak this is the app's
/// cache directory, which is a real path on the host (under
/// `~/.var/app/<app-id>/`), so host-side converters can read and write it.
pub fn temp_dir() -> PathBuf {
    if in_flatpak() {
        let dir = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".cache"))
            .join("converter");
        let _ = std::fs::create_dir_all(&dir);
        dir
    } else {
        std::env::temp_dir()
    }
}

/// Build a converter command for `name`, running in `cwd` when given.
/// Callers add arguments afterwards, exactly as with a direct `Command`.
pub fn command(name: &str, cwd: Option<&Path>) -> Command {
    if !in_flatpak() {
        let mut cmd = Command::new(name);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        return cmd;
    }
    // `flatpak-spawn --host` has no working-directory option, so the `sh`
    // wrapper provides one; `$0` names the script for error messages.
    let script = match cwd {
        Some(dir) => format!("cd {} && exec {} \"$@\"", shell_quote(dir), name),
        None => format!("exec {} \"$@\"", name),
    };
    let mut cmd = Command::new("flatpak-spawn");
    cmd.arg("--host")
        .arg("sh")
        .arg("-c")
        .arg(script)
        .arg(name);
    cmd
}

/// Single-quote a path for the POSIX shell the wrapper runs under.
fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Friendly message for a converter that could not be started at all.
/// Inside Flatpak a missing host converter surfaces as a shell exit code,
/// not a spawn error, so both shapes of failure land on the same wording.
pub fn missing_converter_message() -> String {
    "The document converter (pandoc) is not installed. Install it to use the preview and export."
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quote_escapes_quotes() {
        assert_eq!(shell_quote(Path::new("/home/u/docs")), "'/home/u/docs'");
        assert_eq!(
            shell_quote(Path::new("/home/u/my docs")),
            "'/home/u/my docs'"
        );
        assert_eq!(
            shell_quote(Path::new("/home/u/it's")),
            "'/home/u/it'\\''s'"
        );
    }

    #[test]
    fn command_is_direct_outside_flatpak() {
        // This test suite never runs inside a Flatpak sandbox.
        assert!(!in_flatpak());
        let cmd = command("pandoc", Some(Path::new("/tmp")));
        assert_eq!(cmd.get_program(), "pandoc");
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/tmp")));
    }
}
