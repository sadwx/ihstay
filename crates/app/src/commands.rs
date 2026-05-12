use crate::state::SharedState;
use ihstay_core::config::Config;
use ihstay_core::types::Entry;
use ihstay_core::visibility::{VisibilityAction, VisibilityEvent};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn list_entries(state: State<SharedState>) -> Vec<Entry> {
    let s = state.lock().unwrap();
    s.entries()
}

#[tauri::command]
pub fn focus_entry(state: State<SharedState>, session_id: String) -> Result<String, String> {
    use ihstay_core::types::{EntryState, TerminalMatch};

    // Collect everything we need from state under the lock, then drop it
    // before calling adapter methods (which shell out and can block).
    let (entry, focus_target, adapter_name) = {
        let s = state.lock().unwrap();
        let entry = s
            .store
            .get(&session_id)
            .ok_or_else(|| "entry not found".to_string())?
            .clone();

        // Routing for click-to-focus, in priority order:
        //   1. If the hook captured `$WEZTERM_PANE`, address that pane
        //      directly via the WezTerm adapter. Works for native Windows
        //      (fixes the historical always-pane[0] bug), WSL (claude_pid
        //      is unreachable from the Windows pid namespace, so the walk
        //      can never succeed), and macOS-with-wezterm.
        //   2. If the hook captured a `tty` (iTerm2 path on macOS), use
        //      it directly. The PTY survives claude exit, so this works
        //      even for Stale entries where the recorded claude_pid is
        //      dead — switching to the still-open tab beats spawning a
        //      fresh `claude --resume` tab.
        //   3. Otherwise for live, non-WSL entries try the legacy ancestor
        //      walk so older boards (no stored tty) keep working.
        //   4. WSL entries with no captured pane id fall through to
        //      spawn_resume below (last-resort: opens a fresh tab).
        let focus_target: Option<(String, TerminalMatch)> =
            if let Some(pane_id) = entry.wezterm_pane_id.clone() {
                Some((
                    "WezTerm".to_string(),
                    TerminalMatch {
                        terminal_name: "WezTerm".to_string(),
                        terminal_pid: entry.terminal_pid.unwrap_or(0),
                        pane_id: Some(pane_id),
                        tty: None,
                    },
                ))
            } else if entry.tty.is_some() && entry.wsl_distro.is_none() {
                Some((
                    "iTerm2".to_string(),
                    TerminalMatch {
                        terminal_name: "iTerm2".to_string(),
                        terminal_pid: entry.terminal_pid.unwrap_or(0),
                        pane_id: None,
                        tty: entry.tty.clone(),
                    },
                ))
            } else if entry.state == EntryState::Live && entry.wsl_distro.is_none() {
                s.adapter_registry
                    .detect(entry.claude_pid)
                    .map(|(adapter, m)| (adapter.name().to_string(), m))
            } else {
                None
            };

        let adapter_name = s.config.default_adapter.clone();
        (entry, focus_target, adapter_name)
    };

    if let Some((target_adapter, terminal_match)) = focus_target {
        let focus_result = {
            let s = state.lock().unwrap();
            s.adapter_registry
                .get_by_name(&target_adapter)
                .map(|adapter| adapter.focus_pane(&terminal_match))
        };
        match focus_result {
            Some(Ok(())) => return Ok("focused".to_string()),
            Some(Err(e)) => {
                // The captured pane may have been closed since the hook
                // fired — fall through to spawn_resume so the user lands
                // on a working session instead of an error.
                tracing::warn!(error = %e, "focus_pane failed, falling back to spawn_resume");
            }
            None => {
                tracing::warn!(target_adapter, "focus target adapter not available");
            }
        }
    }

    {
        let s = state.lock().unwrap();
        if let Some(adapter) = s.adapter_registry.get_by_name(&adapter_name) {
            adapter
                .spawn_resume(&entry.cwd, &entry.session_id, entry.wsl_distro.as_deref())
                .map_err(|e| format!("spawn failed: {e}"))?;
            return Ok("resumed".to_string());
        }
    }

    Err("no adapter available".to_string())
}

/// Frontend signals that the user just opened the dismiss confirmation
/// panel. Cancels any pending auto-hide grace deadline so the HUD doesn't
/// vanish before the panel commits — see `VisibilityEvent::DismissPanelOpened`.
#[tauri::command]
pub fn dismiss_panel_opened(state: State<SharedState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.visibility.handle(VisibilityEvent::DismissPanelOpened);
    Ok(())
}

/// Returns the current WezTerm-stale-WSLENV warning state without
/// clearing it. The HUD calls this on init to know whether to render the
/// banner (covering the case where the backend's emit landed before the
/// listener attached). The flag persists until either every PID in
/// `stale_wezterm_pids` exits (auto-dismiss in
/// `wezterm_stale_check_loop`) or the user clicks the banner X
/// (`dismiss_wezterm_stale_warning`).
#[tauri::command]
pub fn wezterm_stale_warning_active(state: State<SharedState>) -> bool {
    let s = state.lock().unwrap();
    s.wezterm_stale_warning
}

/// User clicked the banner X. Clears the warning so the banner stays
/// hidden across HUD reopens, and discards the captured PID list so the
/// check loop doesn't fight the manual dismiss.
#[tauri::command]
pub fn dismiss_wezterm_stale_warning(state: State<SharedState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.wezterm_stale_warning = false;
    s.stale_wezterm_pids.clear();
    Ok(())
}

#[tauri::command]
pub fn dismiss_hud(
    app: AppHandle,
    state: State<SharedState>,
    reminding_override: Option<bool>,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let action = s
        .visibility
        .handle(VisibilityEvent::ManualDismiss { reminding_override });
    drop(s);

    if action == VisibilityAction::HideHud {
        if let Some(window) = app.get_webview_window("hud") {
            let _ = window.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn manual_open(app: AppHandle, state: State<SharedState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let entries = s.entries();
    let action = s.visibility.handle(VisibilityEvent::ManualOpen {
        board_count: entries.len(),
    });
    drop(s);

    if action == VisibilityAction::ShowHud {
        if let Some(window) = app.get_webview_window("hud") {
            crate::hud_show::show_without_activation(&window);
            let _ = app.emit("entries-updated", &entries);
        }
    }
    Ok(())
}

/// Shared helper to show (and focus) the pre-created Settings window.
///
/// The settings window is built up-front during app setup — we just show it here.
/// This avoids the race condition and hang that happened when creating the window
/// on-demand from a Tauri command or tray menu handler.
pub fn open_settings_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window not found".to_string())?;
    window
        .show()
        .map_err(|e| format!("failed to show settings: {e}"))?;
    window
        .set_focus()
        .map_err(|e| format!("failed to focus settings: {e}"))?;
    tracing::info!("settings window shown");
    Ok(())
}

#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    open_settings_window(&app)
}

#[tauri::command]
pub fn hide_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window
            .hide()
            .map_err(|e| format!("failed to hide settings: {e}"))?;
    }
    Ok(())
}

/// Move the HUD window to its default position (bottom-right of the primary
/// monitor, near the tray) and clear any saved position in config.
#[tauri::command]
pub fn reset_hud_position(app: AppHandle, state: State<SharedState>) -> Result<(), String> {
    let window = app
        .get_webview_window("hud")
        .ok_or_else(|| "hud window not found".to_string())?;

    let monitor = window
        .primary_monitor()
        .map_err(|e| format!("failed to get monitor: {e}"))?
        .ok_or_else(|| "no primary monitor".to_string())?;

    let size = monitor.size();
    let scale = monitor.scale_factor();

    // HUD is 380x240 logical pixels. Margin + taskbar allowance at the bottom.
    let hud_w = (380.0 * scale) as i32;
    let hud_h = (240.0 * scale) as i32;
    let margin_right = (16.0 * scale) as i32;
    let margin_bottom = (64.0 * scale) as i32;

    let x = size.width as i32 - hud_w - margin_right;
    let y = size.height as i32 - hud_h - margin_bottom;

    let position = tauri::PhysicalPosition::new(x, y);
    window
        .set_position(position)
        .map_err(|e| format!("failed to set position: {e}"))?;

    let mut s = state.lock().unwrap();
    s.config.hud_position = None;
    s.config
        .save(&Config::default_path())
        .map_err(|e| format!("failed to save config: {e}"))?;

    tracing::info!(x, y, "HUD position reset to tray-anchor default");
    Ok(())
}

#[tauri::command]
pub fn get_config(state: State<SharedState>) -> Config {
    let s = state.lock().unwrap();
    s.config.clone()
}

#[tauri::command]
pub fn apply_config(state: State<SharedState>, config: Config) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    s.visibility.update_config(config.clone());
    config
        .save(&Config::default_path())
        .map_err(|e| format!("failed to save config: {e}"))?;
    s.config = config;
    Ok(())
}

#[tauri::command]
pub fn check_hooks_installed() -> crate::plugin_install::HookStatus {
    crate::plugin_install::detect()
}

#[tauri::command]
pub async fn install_plugin() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(crate::plugin_install::install)
        .await
        .map_err(|e| format!("install task failed: {e}"))?
}

/// Manually dismiss a single entry from the HUD.
///
/// Appends a `clear` op with reason `user_dismissed` to the board file. The
/// watcher picks it up, the store drops the entry, and the HUD re-renders
/// through the normal pipeline — same shape as hook-driven clears or the
/// periodic stale-cleanup loop.
#[tauri::command]
pub fn dismiss_entry(session_id: String) -> Result<(), String> {
    use ihstay_core::types::Op;
    use std::io::Write;

    let home = dirs_next::home_dir().ok_or_else(|| "no home dir".to_string())?;
    let board_file = home.join(".claude").join("pending").join("board.jsonl");

    let op = Op::Clear {
        ts: chrono::Utc::now(),
        session_id,
        reason: "user_dismissed".to_string(),
    };
    let line = serde_json::to_string(&op).map_err(|e| format!("serialize: {e}"))?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&board_file)
        .map_err(|e| format!("open board: {e}"))?;
    writeln!(file, "{}", line).map_err(|e| format!("write: {e}"))?;
    Ok(())
}
