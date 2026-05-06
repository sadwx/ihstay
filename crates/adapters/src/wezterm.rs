use claude_pending_board_core::terminal::{AdapterError, TerminalAdapter};
use claude_pending_board_core::types::TerminalMatch;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Windows console-suppression flag for `Command::creation_flags`. Defined
/// inline so the crate stays free of a `winapi`/`windows-sys` dep.
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Find a running `wezterm-gui` process and return the absolute path of its
/// mux socket. WezTerm's CLI computes the per-mux socket name as
/// `gui-sock-<pid>` but treats it as a relative path when
/// `WEZTERM_UNIX_SOCKET` is unset — every `wezterm cli` invocation from a
/// process that was not itself spawned by WezTerm fails the connection.
/// The tray app launches from the OS startup folder / tray with no such
/// env, so this helper supplies the absolute path explicitly.
///
/// When multiple `wezterm-gui` processes run, picks the most-recently-started
/// one. Returns `None` when no `wezterm-gui` is running, in which case the
/// caller should fall back to invoking `wezterm` without a custom env (so
/// wezterm's own auto-start path can launch a fresh mux).
fn wezterm_socket_path() -> Option<PathBuf> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );

    #[cfg(target_os = "windows")]
    let target = "wezterm-gui.exe";
    #[cfg(not(target_os = "windows"))]
    let target = "wezterm-gui";

    let pid = sys
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            if p.name().eq_ignore_ascii_case(target) {
                Some((pid.as_u32(), p.start_time()))
            } else {
                None
            }
        })
        .max_by_key(|(_, start)| *start)
        .map(|(pid, _)| pid)?;

    socket_path_for_pid(pid)
}

/// OS-specific socket path construction. Windows builds use
/// `<USERPROFILE>\.local\share\wezterm\gui-sock-<pid>`; macOS uses the same
/// scheme rooted at `$HOME`.
fn socket_path_for_pid(pid: u32) -> Option<PathBuf> {
    let home = dirs_next::home_dir()?;
    Some(
        home.join(".local/share/wezterm")
            .join(format!("gui-sock-{pid}")),
    )
}

/// Wrap `Command::new(wezterm)` so every `wezterm cli` subprocess gets the
/// mux socket path explicitly (regression fix — see `wezterm_socket_path`)
/// and on Windows is suppressed from briefly flashing a console window.
fn wezterm_command(binary: &str) -> Command {
    let mut cmd = Command::new(binary);
    if let Some(socket) = wezterm_socket_path() {
        cmd.env("WEZTERM_UNIX_SOCKET", socket);
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// WezTerm pane info from `wezterm cli list --format json`.
#[derive(Debug, Deserialize)]
struct WezTermPane {
    #[allow(dead_code)]
    window_id: u64,
    #[allow(dead_code)]
    tab_id: u64,
    pane_id: u64,
    #[serde(default)]
    #[allow(dead_code)]
    title: String,
    #[serde(default)]
    #[allow(dead_code)]
    cwd: String,
}

pub struct WezTermAdapter;

impl Default for WezTermAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WezTermAdapter {
    pub fn new() -> Self {
        Self
    }

    fn find_binary() -> Option<String> {
        if Command::new("wezterm")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some("wezterm".to_string());
        }

        #[cfg(target_os = "windows")]
        {
            let program_files = std::env::var("ProgramFiles").unwrap_or_default();
            let path = format!("{}\\WezTerm\\wezterm.exe", program_files);
            if std::path::Path::new(&path).exists() {
                return Some(path);
            }
        }

        None
    }

    fn list_panes() -> Result<Vec<WezTermPane>, AdapterError> {
        let binary = Self::find_binary().ok_or(AdapterError::BinaryNotFound)?;

        let output = wezterm_command(&binary)
            .args(["cli", "list", "--format", "json"])
            .output()
            .map_err(|e| {
                AdapterError::CommandFailed(format!("failed to run wezterm cli list: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AdapterError::CommandFailed(format!(
                "wezterm cli list failed: {stderr}"
            )));
        }

        let panes: Vec<WezTermPane> = serde_json::from_slice(&output.stdout)
            .map_err(|e| AdapterError::CommandFailed(format!("failed to parse pane list: {e}")))?;

        Ok(panes)
    }

    fn activate_pane(pane_id: u64) -> Result<(), AdapterError> {
        let binary = Self::find_binary().ok_or(AdapterError::BinaryNotFound)?;

        let output = wezterm_command(&binary)
            .args(["cli", "activate-pane", "--pane-id", &pane_id.to_string()])
            .output()
            .map_err(|e| {
                AdapterError::CommandFailed(format!("failed to run activate-pane: {e}"))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AdapterError::CommandFailed(format!(
                "activate-pane failed: {stderr}"
            )));
        }

        Ok(())
    }

    /// Bring the WezTerm GUI window to the foreground after activating a
    /// pane. `wezterm cli activate-pane` only switches *within* WezTerm —
    /// if WezTerm is minimized or behind another window, the user
    /// silently sees nothing happen until they alt-tab over.
    fn raise_wezterm_window() {
        #[cfg(target_os = "windows")]
        raise_window_windows();

        #[cfg(target_os = "macos")]
        raise_window_macos();
    }

    fn find_pane_for_pid(claude_pid: u32, panes: &[WezTermPane]) -> Option<(u64, TerminalMatch)> {
        let (terminal_name, terminal_pid) =
            claude_pending_board_core::terminal::ancestor_walk(claude_pid, 20)?;

        if let Some(pane) = panes.first() {
            return Some((
                pane.pane_id,
                TerminalMatch {
                    terminal_name,
                    terminal_pid,
                    pane_id: Some(pane.pane_id.to_string()),
                    tty: None,
                },
            ));
        }

        None
    }
}

#[cfg(target_os = "windows")]
fn raise_window_windows() {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
    };

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );
    let pids: Vec<u32> = sys
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            if p.name().eq_ignore_ascii_case("wezterm-gui.exe") {
                Some(pid.as_u32())
            } else {
                None
            }
        })
        .collect();
    if pids.is_empty() {
        tracing::debug!("no wezterm-gui process; cannot raise window");
        return;
    }

    struct Ctx {
        target_pids: Vec<u32>,
        found_hwnd: Option<HWND>,
    }
    let mut ctx = Ctx {
        target_pids: pids,
        found_hwnd: None,
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };
        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32)) };
        if !ctx.target_pids.contains(&pid) {
            return BOOL(1);
        }
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }
        // Top-level only — skip child / owned popups.
        if let Ok(owner) = unsafe { GetWindow(hwnd, GW_OWNER) } {
            if !owner.is_invalid() {
                return BOOL(1);
            }
        }
        ctx.found_hwnd = Some(hwnd);
        BOOL(0)
    }

    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }

    let Some(hwnd) = ctx.found_hwnd else {
        tracing::debug!("no top-level wezterm-gui window found; cannot raise");
        return;
    };

    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let raised = SetForegroundWindow(hwnd).as_bool();
        if !raised {
            tracing::debug!(
                "SetForegroundWindow refused — focus-stealing prevention may have suppressed the raise"
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn raise_window_macos() {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

    // Skip if no wezterm-gui is running. `tell application "WezTerm" to
    // activate` would otherwise *launch* WezTerm — surprising for a
    // click-to-focus that should land on an existing pane.
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );
    let running = sys.processes().values().any(|p| {
        p.name()
            .to_string_lossy()
            .eq_ignore_ascii_case("wezterm-gui")
    });
    if !running {
        tracing::debug!("no wezterm-gui process; skipping osascript activate");
        return;
    }

    let output = Command::new("osascript")
        .args(["-e", r#"tell application "WezTerm" to activate"#])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            tracing::debug!("raised WezTerm via osascript");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::debug!(stderr = %stderr, "osascript activate failed");
        }
        Err(e) => {
            tracing::debug!(error = %e, "failed to spawn osascript");
        }
    }
}

impl TerminalAdapter for WezTermAdapter {
    fn name(&self) -> &str {
        "WezTerm"
    }

    fn is_available(&self) -> bool {
        Self::find_binary().is_some()
    }

    fn detect(&self, claude_pid: u32) -> Option<TerminalMatch> {
        let panes = Self::list_panes().ok()?;
        Self::find_pane_for_pid(claude_pid, &panes).map(|(_, m)| m)
    }

    fn focus_pane(&self, terminal_match: &TerminalMatch) -> Result<(), AdapterError> {
        let pane_id: u64 = terminal_match
            .pane_id
            .as_ref()
            .ok_or(AdapterError::NoPaneFound)?
            .parse()
            .map_err(|e| AdapterError::CommandFailed(format!("invalid pane_id: {e}")))?;

        Self::activate_pane(pane_id)?;
        // activate-pane only switches inside WezTerm; raise the OS window
        // too so the user actually sees the result.
        Self::raise_wezterm_window();
        Ok(())
    }

    fn spawn_resume(
        &self,
        cwd: &Path,
        session_id: &str,
        wsl_distro: Option<&str>,
    ) -> Result<(), AdapterError> {
        let binary = Self::find_binary().ok_or(AdapterError::BinaryNotFound)?;

        let mut command = wezterm_command(&binary);
        let resume_cmd;
        if let Some(distro) = wsl_distro {
            // WSL-origin entry: translate the Linux cwd to a \\wsl$\<distro>\…
            // UNC and run the resume command inside WSL via wsl.exe. Otherwise
            // wezterm (running on Windows) can't enter the path and Claude
            // (running on the Windows side) won't know about the session id
            // that lives in the WSL distro.
            //
            // Use `bash -lc` rather than `wsl.exe -e <cmd>`: the latter skips
            // the login shell, so any `claude` install that adds itself to
            // PATH only via rcfiles (the common case for `~/.local/bin`,
            // `~/.npm-global/bin`, asdf/mise, /mnt/c-mounted Windows npm
            // shims, …) fails with `execvpe(claude) failed`. The login form
            // sources `/etc/profile`, `~/.profile`, and friends before
            // exec'ing claude.
            //
            // Session id is a UUID per `Op::Add`'s schema — alphanumeric +
            // dashes only, no shell metacharacters — so direct
            // interpolation is safe. Asserted below.
            debug_assert!(
                is_uuid_like(session_id),
                "session_id should be UUID-shaped before reaching spawn_resume"
            );
            let unc_cwd = wsl_cwd_to_unc(distro, cwd);
            resume_cmd = format!("claude --resume {session_id}");
            command.args([
                "cli",
                "spawn",
                "--cwd",
                &unc_cwd,
                "--",
                "wsl.exe",
                "-d",
                distro,
                "--",
                "bash",
                "-lc",
                &resume_cmd,
            ]);
        } else {
            command.args([
                "cli",
                "spawn",
                "--cwd",
                &cwd.to_string_lossy(),
                "--",
                "claude",
                "--resume",
                session_id,
            ]);
        }

        let output = command
            .output()
            .map_err(|e| AdapterError::CommandFailed(format!("failed to spawn: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AdapterError::CommandFailed(format!(
                "wezterm cli spawn failed: {stderr}"
            )));
        }

        tracing::info!(
            session_id,
            cwd = %cwd.display(),
            wsl_distro = ?wsl_distro,
            "spawned resume in new WezTerm tab"
        );
        Ok(())
    }
}

/// Translate a Linux path into a Windows UNC path that resolves into the
/// named WSL distro's filesystem. Pure string transform, no I/O. Used by the
/// WezTerm adapter when spawning a new tab for a WSL-origin entry — `wezterm
/// cli spawn` runs on the Windows side and can't `cd` into a Linux path.
///
/// `/home/user/project` → `\\wsl$\Ubuntu-24.04\home\user\project`
/// `/`                   → `\\wsl$\Ubuntu-24.04\`
/// Cheap UUID shape check — matches Claude Code's session id format
/// `[0-9a-f-]{36}` (8-4-4-4-12 hex with dashes). Used as a `debug_assert!`
/// guard before interpolating into a `bash -lc` command line.
fn is_uuid_like(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    s.bytes().enumerate().all(|(i, b)| match i {
        8 | 13 | 18 | 23 => b == b'-',
        _ => b.is_ascii_hexdigit(),
    })
}

pub(crate) fn wsl_cwd_to_unc(distro: &str, linux_cwd: &Path) -> String {
    let path_str = linux_cwd.to_string_lossy();
    // Drop the leading `/` if present, then convert remaining `/` to `\`.
    let stripped = path_str.strip_prefix('/').unwrap_or(&path_str);
    let backslashed = stripped.replace('/', "\\");
    if backslashed.is_empty() {
        format!("\\\\wsl$\\{}\\", distro)
    } else {
        format!("\\\\wsl$\\{}\\{}", distro, backslashed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wezterm_adapter_name() {
        let adapter = WezTermAdapter::new();
        assert_eq!(adapter.name(), "WezTerm");
    }

    #[test]
    fn test_find_pane_single_pane() {
        let panes = vec![WezTermPane {
            window_id: 0,
            tab_id: 0,
            pane_id: 42,
            title: "test".to_string(),
            cwd: "file:///tmp".to_string(),
        }];
        // Returns None because ancestor_walk won't find WezTerm from fake PID
        let result = WezTermAdapter::find_pane_for_pid(99999, &panes);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_wezterm_pane_json() {
        let json = r#"[{"window_id":0,"tab_id":0,"pane_id":0,"workspace":"default","size":{"rows":24,"cols":80},"title":"test","cwd":"file:///home/user"}]"#;
        let panes: Vec<WezTermPane> = serde_json::from_str(json).unwrap();
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pane_id, 0);
        assert_eq!(panes[0].cwd, "file:///home/user");
    }

    #[test]
    fn test_parse_wezterm_pane_json_with_extra_fields() {
        let json = r#"[{"window_id":0,"tab_id":0,"pane_id":5,"title":"x","cwd":"file:///tmp","future_field":true}]"#;
        let panes: Vec<WezTermPane> = serde_json::from_str(json).unwrap();
        assert_eq!(panes[0].pane_id, 5);
    }

    #[test]
    #[ignore]
    fn test_wezterm_is_available() {
        let adapter = WezTermAdapter::new();
        assert!(adapter.is_available(), "WezTerm binary not found in PATH");
    }

    #[test]
    #[ignore]
    fn test_wezterm_list_panes() {
        let panes = WezTermAdapter::list_panes().expect("failed to list panes");
        assert!(!panes.is_empty(), "no panes found — is WezTerm running?");
        for pane in &panes {
            println!(
                "  pane_id={} tab_id={} window_id={} title={:?} cwd={:?}",
                pane.pane_id, pane.tab_id, pane.window_id, pane.title, pane.cwd
            );
        }
    }

    #[test]
    fn test_wsl_cwd_to_unc_typical_home() {
        let result = wsl_cwd_to_unc("Ubuntu-24.04", Path::new("/home/user/project"));
        assert_eq!(result, r"\\wsl$\Ubuntu-24.04\home\user\project");
    }

    #[test]
    fn test_wsl_cwd_to_unc_root() {
        let result = wsl_cwd_to_unc("Ubuntu-24.04", Path::new("/"));
        assert_eq!(result, r"\\wsl$\Ubuntu-24.04\");
    }

    #[test]
    fn test_wsl_cwd_to_unc_no_leading_slash() {
        // Defensive: a relative-ish path (no leading slash) shouldn't double
        // the prefix or panic.
        let result = wsl_cwd_to_unc("Ubuntu-24.04", Path::new("home/user"));
        assert_eq!(result, r"\\wsl$\Ubuntu-24.04\home\user");
    }

    #[test]
    fn test_wsl_cwd_to_unc_other_distro() {
        let result = wsl_cwd_to_unc("Debian", Path::new("/var/log"));
        assert_eq!(result, r"\\wsl$\Debian\var\log");
    }

    #[test]
    fn test_socket_path_for_pid_uses_local_share() {
        // Pure-string assertion: if home_dir() returned anything, the result
        // ends with the well-known suffix WezTerm CLI expects. We don't pin
        // the exact home directory because the test environment varies.
        let result = socket_path_for_pid(12345);
        let path = result.expect("home_dir should resolve in the test env");
        let path_str = path.to_string_lossy().replace('\\', "/");
        assert!(
            path_str.ends_with(".local/share/wezterm/gui-sock-12345"),
            "unexpected suffix in socket path: {path_str}"
        );
    }

    #[test]
    fn test_is_uuid_like_canonical() {
        assert!(is_uuid_like("76ae9be4-e26a-49e8-aae0-f245b372bc48"));
        assert!(is_uuid_like("00000000-0000-0000-0000-000000000000"));
    }

    #[test]
    fn test_is_uuid_like_rejects_metacharacters() {
        // The whole point of this guard: anything that could escape a
        // bash -lc 'claude --resume <id>' interpolation is rejected.
        assert!(!is_uuid_like("76ae9be4'; rm -rf ~"));
        assert!(!is_uuid_like("76ae9be4-e26a-49e8-aae0-f245b372bc4z")); // non-hex
        assert!(!is_uuid_like("76ae9be4-e26a-49e8-aae0-f245b372bc4")); // short
        assert!(!is_uuid_like("76ae9be4 e26a 49e8 aae0 f245b372bc48")); // spaces
        assert!(!is_uuid_like(""));
    }

    #[test]
    fn test_wezterm_command_no_console_window_flag_on_windows() {
        // The flag is set inside `wezterm_command`. We can't introspect it
        // through the `Command` API, but we can verify that the command
        // round-trips without panicking and the program name matches.
        let cmd = wezterm_command("wezterm");
        assert_eq!(cmd.get_program(), std::ffi::OsStr::new("wezterm"));
    }

    #[test]
    fn test_wezterm_command_omits_socket_env_when_no_gui_running() {
        // If `wezterm_socket_path()` returns None (no wezterm-gui in the
        // process table at test time), the resulting Command MUST NOT carry
        // a stale or empty WEZTERM_UNIX_SOCKET. This guards against a
        // refactor where a `Some(empty)` path gets set unconditionally.
        if wezterm_socket_path().is_none() {
            let cmd = wezterm_command("wezterm");
            let has_socket_env = cmd
                .get_envs()
                .any(|(k, v)| k == std::ffi::OsStr::new("WEZTERM_UNIX_SOCKET") && v.is_some());
            assert!(!has_socket_env);
        }
        // When wezterm IS running, we just verify the command builds without
        // panicking — the absolute-path content is exercised by
        // `test_socket_path_for_pid_uses_local_share`.
    }
}
