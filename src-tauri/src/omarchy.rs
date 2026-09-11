//! Omarchy integration: detect the desktop, and add or remove the miniti bar
//! widget through Omarchy's own plugin commands. The widget lives in its own
//! repository (the marketplace clones one repo per plugin); the app just
//! offers the one click. With the widget in the bar the tray icon is
//! redundant, so the app hides it and keeps running in the background when
//! the window closes (the widget raises it).

use serde::Serialize;
use std::path::PathBuf;

pub const PLUGIN_ID: &str = "io.github.12ian34.miniti";
pub const PLUGIN_REPO: &str = "https://github.com/12ian34/miniti-omarchy.git";

#[derive(Debug, Clone, Serialize)]
pub struct OmarchyStatus {
    pub is_omarchy: bool,
    pub widget_installed: bool,
}

fn command_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

/// Omarchy sets `OMARCHY_PATH` in every session and ships the `omarchy` CLI.
pub fn is_omarchy() -> bool {
    std::env::var_os("OMARCHY_PATH").is_some() || command_on_path("omarchy")
}

pub fn plugin_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omarchy")
        .join("plugins")
        .join(PLUGIN_ID)
}

pub fn widget_installed() -> bool {
    plugin_dir().join("manifest.json").is_file()
}

pub fn status() -> OmarchyStatus {
    OmarchyStatus {
        is_omarchy: is_omarchy(),
        widget_installed: widget_installed(),
    }
}

fn run(args: &[&str]) -> Result<(), String> {
    if !command_on_path("omarchy") {
        return Err("the omarchy command is not on this machine".into());
    }
    let out = std::process::Command::new("omarchy")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("could not run omarchy: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let detail = stderr.trim().lines().last().or_else(|| stdout.trim().lines().last()).unwrap_or("");
    Err(format!("omarchy {} failed: {}", args.join(" "), detail))
}

/// Clone the widget into the plugins folder and put it in the bar.
pub fn install_widget() -> Result<(), String> {
    if widget_installed() {
        return run(&["plugin", "enable", PLUGIN_ID]).or(Ok(()));
    }
    run(&["plugin", "add", PLUGIN_REPO, "--enable", "--yes"])
}

pub fn remove_widget() -> Result<(), String> {
    if !widget_installed() {
        return Ok(());
    }
    run(&["plugin", "remove", PLUGIN_ID, "--yes"])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_dir_is_under_the_omarchy_config() {
        let p = plugin_dir();
        assert!(p.ends_with(format!("omarchy/plugins/{PLUGIN_ID}")), "{}", p.display());
    }
}
