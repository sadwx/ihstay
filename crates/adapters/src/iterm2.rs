#![cfg(target_os = "macos")]

use ihstay_core::terminal::{AdapterError, TerminalAdapter};
use ihstay_core::types::TerminalMatch;
use std::path::Path;
use std::process::Command;

pub struct ITerm2Adapter;

impl ITerm2Adapter {
    pub fn new() -> Self {
        Self
    }

    fn is_iterm2_installed() -> bool {
        Path::new("/Applications/iTerm.app").exists()
    }

    fn run_osascript(script: &str) -> Result<String, AdapterError> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| AdapterError::CommandFailed(format!("osascript failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AdapterError::CommandFailed(format!(
                "osascript error: {stderr}"
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn get_tty(pid: u32) -> Option<String> {
        let output = Command::new("ps")
            .args(["-o", "tty=", "-p", &pid.to_string()])
            .output()
            .ok()?;

        if output.status.success() {
            let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !tty.is_empty() && tty != "??" {
                return Some(tty);
            }
        }
        None
    }
}

impl Default for ITerm2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalAdapter for ITerm2Adapter {
    fn name(&self) -> &str {
        "iTerm2"
    }

    fn is_available(&self) -> bool {
        Self::is_iterm2_installed()
    }

    fn detect(&self, claude_pid: u32) -> Option<TerminalMatch> {
        let (terminal_name, terminal_pid) = ihstay_core::terminal::ancestor_walk(claude_pid, 20)?;

        if !terminal_name.contains("iTerm") {
            return None;
        }

        let tty = Self::get_tty(claude_pid);

        Some(TerminalMatch {
            terminal_name,
            terminal_pid,
            pane_id: None,
            tty,
        })
    }

    fn focus_pane(&self, terminal_match: &TerminalMatch) -> Result<(), AdapterError> {
        // No tty means we can't address a specific session — bring iTerm2
        // forward and report no-pane so the caller can decide whether to
        // fall back to spawn_resume.
        let Some(tty) = &terminal_match.tty else {
            Self::run_osascript(r#"tell application "iTerm2" to activate"#)?;
            return Err(AdapterError::NoPaneFound);
        };

        // Single AppleScript invocation: activate iTerm2, find the session
        // by tty, raise its window (`set index of w to 1` — required when
        // multiple iTerm2 windows are open, otherwise `select` only
        // switches the tab within whichever window happens to be
        // frontmost), then select tab + session. Returns "found" or
        // "not_found".
        let script = format!(
            r#"tell application "iTerm2"
    activate
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                if tty of s contains "{tty}" then
                    set index of w to 1
                    select t
                    select s
                    return "found"
                end if
            end repeat
        end repeat
    end repeat
    return "not_found"
end tell"#,
            tty = tty
        );
        let result = Self::run_osascript(&script)?;
        if result == "not_found" {
            tracing::warn!(tty, "iTerm2 session with matching tty not found");
            return Err(AdapterError::NoPaneFound);
        }
        Ok(())
    }

    fn spawn_resume(
        &self,
        cwd: &Path,
        session_id: &str,
        // iTerm2 only ships on macOS where WSL doesn't apply; the field is
        // accepted for trait compatibility and ignored.
        _wsl_distro: Option<&str>,
    ) -> Result<(), AdapterError> {
        // Earlier versions passed `cd ... && claude --resume ...` as the
        // `command` argument to iTerm2's `create tab`. iTerm2 exec()s that
        // string directly without any shell, so `cd` (a builtin) and `&&`
        // are not understood — iTerm2 reports "session ended very soon
        // after starting" and the tab disappears. Instead, create the
        // tab with the user's default profile (which launches their
        // shell) and feed each line via `write text`.
        let cd_line = format!("cd {}", shell_double_quote(&cwd.to_string_lossy()));
        let resume_line = format!("claude --resume {session_id}");

        let script = format!(
            r#"tell application "iTerm2"
    activate
    if (count of windows) is 0 then
        create window with default profile
    end if
    tell current window
        set newTab to (create tab with default profile)
        tell current session of newTab
            write text "{cd}"
            write text "{resume}"
        end tell
    end tell
end tell"#,
            cd = applescript_escape(&cd_line),
            resume = applescript_escape(&resume_line)
        );
        Self::run_osascript(&script)?;

        tracing::info!(session_id, cwd = %cwd.display(), "spawned resume in new iTerm2 tab");
        Ok(())
    }
}

fn shell_double_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iterm2_adapter_name() {
        let adapter = ITerm2Adapter::new();
        assert_eq!(adapter.name(), "iTerm2");
    }

    #[test]
    fn test_detect_returns_none_for_fake_pid() {
        let adapter = ITerm2Adapter::new();
        assert!(adapter.detect(0xFFFFFF).is_none());
    }

    #[test]
    #[ignore]
    fn test_iterm2_is_available() {
        let adapter = ITerm2Adapter::new();
        assert!(
            adapter.is_available(),
            "iTerm2 not found at /Applications/iTerm.app"
        );
    }

    #[test]
    fn test_shell_double_quote() {
        assert_eq!(shell_double_quote("/Users/x"), r#""/Users/x""#);
        assert_eq!(shell_double_quote("with space"), r#""with space""#);
        assert_eq!(shell_double_quote(r#"a"b"#), r#""a\"b""#);
        assert_eq!(shell_double_quote(r"a\b"), r#""a\\b""#);
        assert_eq!(shell_double_quote("a$b"), r#""a\$b""#);
        assert_eq!(shell_double_quote("a`b"), r#""a\`b""#);
    }

    #[test]
    fn test_applescript_escape() {
        assert_eq!(applescript_escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(applescript_escape(r"a\b"), r"a\\b");
        assert_eq!(applescript_escape("plain"), "plain");
    }
}
