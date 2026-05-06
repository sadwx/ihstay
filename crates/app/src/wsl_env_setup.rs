//! Auto-configure `WSLENV` so the env vars that click-to-focus and the bash
//! hook depend on cross the Windows→WSL boundary:
//!
//! - `WEZTERM_PANE/u` — lets the bash hook capture the WezTerm pane id at
//!   notification time so click-to-focus can `wezterm cli activate-pane`
//!   directly instead of falling back to spawn-a-new-tab.
//! - `USERPROFILE/up` — translates `C:\Users\<winuser>` to
//!   `/mnt/c/Users/<winuser>` when crossing into WSL, so the bash hook can
//!   resolve `$USERPROFILE/.claude/pending` and write entries to the
//!   Windows-side board file directly. Removes the per-distro symlink that
//!   was previously the only way to surface multi-distro WSL entries in
//!   the Windows tray.
//!
//! Runs at every app launch in a `spawn_blocking` task; idempotent — once
//! every token is in `WSLENV`, subsequent runs are a single registry read
//! and a debug log.

#![cfg(target_os = "windows")]

use std::os::windows::process::CommandExt;
use std::process::Command;

/// Tokens we ensure are present in the user's `WSLENV`. Order matters only
/// for the deterministic-write case — within an existing user value we
/// append in this order, dedup-skipping any that are already present.
const TOKENS: &[&str] = &["WEZTERM_PANE/u", "USERPROFILE/up"];
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Outcome of a single `ensure_wsl_env_tokens` run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// HKCU already contained every token we care about; nothing was written.
    Unchanged,
    /// HKCU was rewritten with a new value — any process launched before
    /// this point (notably wezterm-gui) is now running with stale env.
    Updated,
    /// WSL not detected, or the registry write failed; no env change took
    /// effect this run.
    NoOp,
}

/// Idempotent setup of every required WSLENV token. Renamed from the older
/// `ensure_wezterm_pane_in_wslenv` to reflect the multi-token reality.
pub fn ensure_wsl_env_tokens() -> Status {
    if !wsl_detected() {
        tracing::debug!("WSL not detected; skipping WSLENV setup");
        return Status::NoOp;
    }

    // Windows merges WSLENV from HKLM (machine) and HKCU (user) when launching
    // new processes — but for non-PATH vars, USER overrides MACHINE entirely.
    // So if we only wrote HKCU we'd silently clobber any system-level tokens
    // (e.g. `JRE_HOME/p`). Read both, merge, and write the union to HKCU.
    let user_wslenv = read_user_wslenv();
    let machine_wslenv = read_machine_wslenv();
    match merge_wslenv(user_wslenv.as_deref(), machine_wslenv.as_deref(), TOKENS) {
        Some(new_value) => {
            tracing::info!(
                user_old = %user_wslenv.as_deref().unwrap_or("(unset)"),
                machine = %machine_wslenv.as_deref().unwrap_or("(unset)"),
                user_new = %new_value,
                "appending missing WSLENV tokens to user WSLENV"
            );
            match write_user_wslenv(&new_value) {
                Ok(()) => Status::Updated,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to update WSLENV — click-to-focus inside WSL will fall back to spawn-a-new-tab");
                    Status::NoOp
                }
            }
        }
        None => {
            tracing::debug!("WSLENV already includes every required token; nothing to do");
            Status::Unchanged
        }
    }
}

/// PIDs of every currently-running `wezterm-gui.exe` process.
///
/// Used after a successful `Updated` to (a) decide whether to surface the
/// "restart WezTerm" warning (empty `Vec` → nothing to warn about) and
/// (b) record which specific instances were stale so the periodic check
/// loop can detect when they've all exited and auto-dismiss the banner.
pub fn wezterm_gui_pids() -> Vec<u32> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );
    sys.processes()
        .iter()
        .filter_map(|(pid, p)| {
            if p.name().eq_ignore_ascii_case("wezterm-gui.exe") {
                Some(pid.as_u32())
            } else {
                None
            }
        })
        .collect()
}

/// True if any of the given PIDs is still alive in the process table. We
/// don't re-check the process name — a recycled PID would be a false
/// positive but the chance over a few minutes is negligible, and erring
/// on "leave the warning up" is the conservative choice.
pub fn any_pid_alive(pids: &[u32]) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    let lookup: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&lookup),
        true,
        ProcessRefreshKind::everything(),
    );
    pids.iter()
        .any(|p| sys.process(Pid::from_u32(*p)).is_some())
}

fn wsl_detected() -> bool {
    let Ok(output) = Command::new("wsl.exe")
        .args(["-l", "-q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    // Treat any non-whitespace byte in stdout as "at least one distro". `wsl
    // -l -q` emits UTF-16 LE with a BOM, so a string parse would need
    // decoding — but we only care about presence, not content.
    output.stdout.iter().any(|b| !b.is_ascii_whitespace())
}

fn read_user_wslenv() -> Option<String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = hkcu.open_subkey("Environment").ok()?;
    env.get_value::<String, _>("WSLENV").ok()
}

fn read_machine_wslenv() -> Option<String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let env = hklm
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
        .ok()?;
    env.get_value::<String, _>("WSLENV").ok()
}

fn write_user_wslenv(new_value: &str) -> Result<(), String> {
    // Shell out to PowerShell rather than writing the registry directly so
    // the .NET SetEnvironmentVariable wrapper handles the WM_SETTINGCHANGE
    // broadcast for us. This only runs the once when we actually need to
    // change the value.
    let escaped = new_value.replace('\'', "''");
    let script = format!("[Environment]::SetEnvironmentVariable('WSLENV', '{escaped}', 'User')");
    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| format!("failed to spawn powershell: {e}"))?;
    if !status.success() {
        return Err(format!("powershell exited {status}"));
    }
    Ok(())
}

/// Pure-string merge: figure out what to write to `HKCU\Environment\WSLENV`.
///
/// Returns `Some(new_value)` if a write is needed, `None` if the user-level
/// value already contains every requested token (idempotent re-run).
///
/// The trick: on first run the user-level value may be empty while the
/// machine-level value carries existing tokens (e.g. `JRE_HOME/p` set by an
/// installer). For non-PATH env vars Windows resolves USER OVER MACHINE at
/// process launch — so writing only `WEZTERM_PANE/u` to HKCU would clobber
/// the machine tokens for new processes. To preserve them, when HKCU is
/// empty we seed the new value with the machine value before appending the
/// missing tokens. Subsequent runs see HKCU is non-empty and respect that
/// as-is.
fn merge_wslenv(user: Option<&str>, machine: Option<&str>, tokens: &[&str]) -> Option<String> {
    // Idempotency: if HKCU already has every requested token we're done. We
    // deliberately don't inspect HKLM here — if the machine value carries a
    // token the user has effectively configured WSLENV manually and our HKCU
    // write would only overwrite their preference.
    if user.is_some_and(|u| tokens.iter().all(|t| contains_token(u, t))) {
        return None;
    }

    // Pick the seed: HKCU if non-empty, otherwise HKLM, otherwise empty.
    let seed = user
        .filter(|u| !u.is_empty())
        .or_else(|| machine.filter(|m| !m.is_empty()))
        .unwrap_or("");

    let mut value = seed.trim_end_matches(':').to_string();
    for token in tokens {
        if contains_token(&value, token) {
            continue;
        }
        if value.is_empty() {
            value.push_str(token);
        } else {
            value.push(':');
            value.push_str(token);
        }
    }
    Some(value)
}

fn contains_token(value: &str, token: &str) -> bool {
    value.split(':').any(|t| !t.is_empty() && t == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper: the token list under test, matching the production constant.
    const T: &[&str] = &["WEZTERM_PANE/u", "USERPROFILE/up"];

    #[test]
    fn merge_into_empty_when_neither_set() {
        assert_eq!(
            merge_wslenv(None, None, T),
            Some("WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
        assert_eq!(
            merge_wslenv(Some(""), Some(""), T),
            Some("WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_when_user_has_all_tokens_is_noop() {
        assert_eq!(
            merge_wslenv(Some("WEZTERM_PANE/u:USERPROFILE/up"), None, T),
            None
        );
        assert_eq!(
            merge_wslenv(Some("FOO/p:WEZTERM_PANE/u:USERPROFILE/up"), None, T),
            None
        );
        assert_eq!(
            merge_wslenv(
                Some("USERPROFILE/up:WEZTERM_PANE/u:BAR"),
                Some("anything"),
                T
            ),
            None
        );
    }

    #[test]
    fn merge_appends_only_missing_tokens() {
        // User already has WEZTERM_PANE/u — append USERPROFILE/up only.
        assert_eq!(
            merge_wslenv(Some("WEZTERM_PANE/u"), None, T),
            Some("WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
        // User has USERPROFILE/up only — append WEZTERM_PANE/u only.
        assert_eq!(
            merge_wslenv(Some("USERPROFILE/up"), None, T),
            Some("USERPROFILE/up:WEZTERM_PANE/u".to_string())
        );
        // Existing unrelated tokens preserved, both new tokens appended.
        assert_eq!(
            merge_wslenv(Some("FOO:BAR/p"), Some("ignored"), T),
            Some("FOO:BAR/p:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_seeds_from_machine_when_user_unset() {
        // The original bug from `WEZTERM_PANE/u`-only days: writing just our
        // tokens to HKCU would clobber the machine-level `JRE_HOME/p` for new
        // processes (USER wins over MACHINE). Seed with the machine value
        // first, then append our missing tokens.
        assert_eq!(
            merge_wslenv(None, Some("JRE_HOME/p"), T),
            Some("JRE_HOME/p:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
        assert_eq!(
            merge_wslenv(Some(""), Some("JRE_HOME/p:OTHER/p"), T),
            Some("JRE_HOME/p:OTHER/p:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_machine_value_already_has_one_of_our_tokens() {
        // Machine has WEZTERM_PANE/u but user is empty. We still need to
        // write to HKCU because USER-empty + MACHINE-set means USER wins as
        // empty otherwise. Don't double up the token; do append USERPROFILE/up.
        assert_eq!(
            merge_wslenv(None, Some("WEZTERM_PANE/u"), T),
            Some("WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
        assert_eq!(
            merge_wslenv(None, Some("FOO:WEZTERM_PANE/u"), T),
            Some("FOO:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_strips_trailing_colon_to_avoid_empty_token() {
        assert_eq!(
            merge_wslenv(Some("FOO:"), None, T),
            Some("FOO:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_does_not_match_partial_tokens() {
        // "WEZTERM_PANE" without the /u suffix is a different token.
        assert_eq!(
            merge_wslenv(Some("WEZTERM_PANE"), None, T),
            Some("WEZTERM_PANE:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
        // Substring shouldn't match either.
        assert_eq!(
            merge_wslenv(Some("OTHER_WEZTERM_PANE/u_THING"), None, T),
            Some("OTHER_WEZTERM_PANE/u_THING:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_handles_leading_colon() {
        assert_eq!(
            merge_wslenv(Some(":FOO"), None, T),
            Some(":FOO:WEZTERM_PANE/u:USERPROFILE/up".to_string())
        );
    }

    #[test]
    fn merge_single_token_list_still_works() {
        // Defensive: the function takes &[&str], so a single-element slice
        // should round-trip without needing the callers to special-case.
        assert_eq!(
            merge_wslenv(None, None, &["WEZTERM_PANE/u"]),
            Some("WEZTERM_PANE/u".to_string())
        );
        assert_eq!(
            merge_wslenv(Some("WEZTERM_PANE/u"), None, &["WEZTERM_PANE/u"]),
            None
        );
    }
}
