//! Detect whether Claude Code's pending-board plugin is installed and, when
//! asked, install it by shelling out to the `claude` CLI.
//!
//! Architecture note: the tray app installer only drops the binary — it does
//! not touch `~/.claude/settings.json`. The hooks are owned by the Claude
//! Code plugin system, so we drive install via `claude plugin marketplace
//! add ...` + `claude plugin install ...`. This runs as the current user (not
//! the MSI's SYSTEM context) because we shell out from the already-running
//! tray process.

use std::path::PathBuf;
use std::process::{Command, Stdio};

// The CLI's `plugin marketplace add` accepts owner/repo, a full URL, or a
// local path — NOT the `github:owner/repo` short-form the Claude Code
// slash command accepts. See `claude plugin marketplace add --help`.
const MARKETPLACE: &str = "sadwx/ihstay";
const PLUGIN_REF: &str = "ihstay@ihstay";
const PLUGIN_NAME: &str = "ihstay";

/// `process.platform`-style identifier for the current OS as used in the
/// plugin's `plugin.json` `platform` annotations.
fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

#[derive(Debug, serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    /// Plugin is installed and enabled.
    Installed,
    /// Plugin is not installed (but `claude` CLI is present).
    NotInstalled,
    /// `claude` CLI is not in PATH — user needs to install Claude Code first.
    CliMissing,
}

pub fn detect() -> HookStatus {
    let Some(output) = run_claude(&["plugin", "list"]) else {
        return HookStatus::CliMissing;
    };
    if !output.status.success() {
        // `claude plugin list` failing usually means auth / init issues, not
        // a missing CLI. Treat as not installed so the user can retry.
        return HookStatus::NotInstalled;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains(PLUGIN_NAME) {
        HookStatus::Installed
    } else {
        HookStatus::NotInstalled
    }
}

pub fn install() -> Result<(), String> {
    let Some(add_output) = run_claude(&["plugin", "marketplace", "add", MARKETPLACE]) else {
        return Err(cli_missing_msg());
    };
    if !add_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_output.stderr);
        // Idempotent: "already exists" from a prior install is fine.
        if !stderr.to_ascii_lowercase().contains("already") {
            return Err(format!(
                "claude plugin marketplace add failed: {}",
                stderr.trim()
            ));
        }
    }

    let Some(install_output) = run_claude(&["plugin", "install", PLUGIN_REF]) else {
        return Err(cli_missing_msg());
    };
    if !install_output.status.success() {
        let stderr = String::from_utf8_lossy(&install_output.stderr);
        return Err(format!("claude plugin install failed: {}", stderr.trim()));
    }

    // Strip OS-irrelevant hook entries from the just-installed plugin.json.
    // Claude Code 2.1.x ignores the `platform` field on hook entries (it's
    // only honored under `mcp_config.platform_overrides`), so without this
    // post-install step the user sees pwsh/bash hooks for *every* OS in
    // `/hooks` and Claude Code attempts to spawn each of them — pwsh
    // ENOENT's on macOS/Linux, bash ENOENT's on plain cmd.exe Windows.
    if let Err(e) = sanitize_installed_plugin_json() {
        // Non-fatal: install succeeded, the irrelevant entries just won't
        // be stripped. Log and move on.
        tracing::warn!(error = %e, "post-install plugin.json sanitize failed");
    }

    Ok(())
}

/// Find the on-disk plugin.json that Claude Code loads from
/// (`~/.claude/plugins/cache/<marketplace>/<plugin>/<version>/.claude-plugin/plugin.json`)
/// and rewrite it to drop hook entries whose `platform` doesn't match the
/// current OS. Returns the number of removed entries (0 if already clean).
pub fn sanitize_installed_plugin_json() -> Result<usize, String> {
    let path = locate_installed_plugin_json()?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read {path:?}: {e}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {path:?}: {e}"))?;

    let removed = strip_foreign_platform_hooks(&mut value, current_platform());
    if removed == 0 {
        return Ok(0);
    }

    let pretty =
        serde_json::to_string_pretty(&value).map_err(|e| format!("serialize plugin.json: {e}"))?;
    std::fs::write(&path, pretty).map_err(|e| format!("write {path:?}: {e}"))?;
    tracing::info!(removed, ?path, "stripped foreign-platform hook entries");
    Ok(removed)
}

fn locate_installed_plugin_json() -> Result<PathBuf, String> {
    let home = dirs_next::home_dir().ok_or_else(|| "no home dir".to_string())?;
    let base = home
        .join(".claude")
        .join("plugins")
        .join("cache")
        .join(PLUGIN_NAME)
        .join(PLUGIN_NAME);
    let entries = std::fs::read_dir(&base).map_err(|e| format!("read {base:?}: {e}"))?;

    // Pick the most-recently-modified version directory. Claude Code keeps
    // multiple versions side-by-side after upgrades; the newest one is the
    // one in use.
    let newest = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .ok_or_else(|| format!("no version dirs under {base:?}"))?;

    Ok(newest.path().join(".claude-plugin").join("plugin.json"))
}

/// Walks the parsed plugin.json and removes any element of any
/// `hooks.<event>[].hooks[]` array whose `platform` field is set and does
/// NOT equal `keep`. Returns the count of removed entries.
fn strip_foreign_platform_hooks(value: &mut serde_json::Value, keep: &str) -> usize {
    let mut removed = 0;
    let Some(hooks) = value.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return 0;
    };
    for (_event, group) in hooks.iter_mut() {
        let Some(group_arr) = group.as_array_mut() else {
            continue;
        };
        for entry in group_arr.iter_mut() {
            let Some(inner) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
                continue;
            };
            inner.retain(|h| match h.get("platform").and_then(|p| p.as_str()) {
                Some(p) if p != keep => {
                    removed += 1;
                    false
                }
                _ => true,
            });
        }
    }
    removed
}

fn run_claude(args: &[&str]) -> Option<std::process::Output> {
    match Command::new("claude")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(o) => Some(o),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            tracing::warn!(error = %e, args = ?args, "`claude` invocation failed");
            None
        }
    }
}

fn cli_missing_msg() -> String {
    "`claude` CLI not found in PATH. Install Claude Code first, then try again.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strip_keeps_only_matching_platform() {
        let mut v = json!({
            "hooks": {
                "Notification": [{
                    "matcher": "",
                    "hooks": [
                        {"type": "command", "command": "pwsh ...", "platform": "windows"},
                        {"type": "command", "command": "bash ...", "platform": "darwin"},
                        {"type": "command", "command": "bash ...", "platform": "linux"}
                    ]
                }]
            }
        });
        let n = strip_foreign_platform_hooks(&mut v, "darwin");
        assert_eq!(n, 2);
        let inner = v["hooks"]["Notification"][0]["hooks"].as_array().unwrap();
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0]["platform"], "darwin");
    }

    #[test]
    fn strip_leaves_unannotated_entries_alone() {
        // Hooks without a `platform` field run on all OSes — must be kept.
        let mut v = json!({
            "hooks": {
                "Stop": [{
                    "matcher": "",
                    "hooks": [
                        {"type": "command", "command": "echo cross-platform"}
                    ]
                }]
            }
        });
        let n = strip_foreign_platform_hooks(&mut v, "darwin");
        assert_eq!(n, 0);
        assert_eq!(v["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn strip_handles_no_hooks_section() {
        let mut v = json!({"name": "test", "version": "1.0.0"});
        assert_eq!(strip_foreign_platform_hooks(&mut v, "darwin"), 0);
    }

    #[test]
    fn strip_walks_multiple_events() {
        let mut v = json!({
            "hooks": {
                "Notification": [{
                    "matcher": "",
                    "hooks": [
                        {"command": "a", "platform": "windows"},
                        {"command": "b", "platform": "darwin"}
                    ]
                }],
                "Stop": [{
                    "matcher": "",
                    "hooks": [
                        {"command": "c", "platform": "linux"},
                        {"command": "d", "platform": "darwin"}
                    ]
                }]
            }
        });
        let n = strip_foreign_platform_hooks(&mut v, "darwin");
        assert_eq!(n, 2);
        assert_eq!(
            v["hooks"]["Notification"][0]["hooks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(v["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
    }
}
