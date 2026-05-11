use crate::state::SharedState;
use chrono::{Duration, Utc};
use ihstay_core::board::compaction;
use ihstay_core::board::watcher::BoardWatcher;
use ihstay_core::reaper::{self, RealProcessTable, RealSessionFiles};
use ihstay_core::types::{EntryState, Op};
use ihstay_core::visibility::{VisibilityAction, VisibilityEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

/// How long a stale entry survives before we emit an automatic clear op for
/// it. Chosen short so orphaned entries (e.g. from sessions the user abandoned
/// and restarted elsewhere with a different session_id) stop cluttering the
/// HUD within the hour.
const STALE_TTL: Duration = Duration::hours(1);

/// How often the stale cleanup loop sweeps the store.
const STALE_CLEANUP_INTERVAL_SECS: u64 = 10 * 60;

fn board_path() -> PathBuf {
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claude").join("pending").join("board.jsonl")
}

pub fn boot(app: &AppHandle, state: SharedState) {
    let app_handle = app.clone();

    let (op_tx, op_rx) = mpsc::unbounded_channel();
    let board_file = board_path();

    if board_file.exists() {
        if let Err(e) = compaction::compact(&board_file, STALE_TTL) {
            tracing::warn!(error = %e, "startup compaction failed");
        }
    }

    match BoardWatcher::start(board_file, op_tx) {
        Ok(watcher) => {
            tracing::info!("board watcher started");
            std::mem::forget(watcher);
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to start board watcher");
        }
    }

    // Watch the plugin cache for marketplace install/update events. When a
    // new version of our plugin lands (e.g. via `claude plugin update`
    // while the tray app is already running), re-run sanitize so the
    // duplicate per-OS hook entries get stripped without requiring an app
    // restart. The boot-time sanitize earlier in setup() handles the
    // already-installed case; this watcher closes the live-update gap.
    let on_cache_change: crate::plugin_watch::OnChange =
        Arc::new(
            || match crate::plugin_install::sanitize_installed_plugin_json() {
                Ok(0) => {}
                Ok(n) => tracing::info!(
                    removed = n,
                    "auto-sanitize: stripped foreign-platform hook entries"
                ),
                Err(e) => tracing::warn!(error = %e, "auto-sanitize failed"),
            },
        );
    match crate::plugin_watch::PluginCacheWatcher::start_default(on_cache_change) {
        Ok(watcher) => {
            std::mem::forget(watcher);
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to start plugin cache watcher");
        }
    }

    let app_for_ops = app_handle.clone();
    let state_for_ops = state.clone();
    tauri::async_runtime::spawn(async move {
        process_ops(op_rx, app_for_ops, state_for_ops).await;
    });

    let app_for_reaper = app_handle.clone();
    let state_for_reaper = state.clone();
    tauri::async_runtime::spawn(async move {
        reaper_loop(app_for_reaper, state_for_reaper).await;
    });

    let app_for_tick = app_handle.clone();
    let state_for_tick = state.clone();
    tauri::async_runtime::spawn(async move {
        visibility_tick_loop(app_for_tick, state_for_tick).await;
    });

    #[cfg(target_os = "windows")]
    {
        let app_for_wezterm_check = app_handle;
        let state_for_wezterm_check = state.clone();
        tauri::async_runtime::spawn(async move {
            wezterm_stale_check_loop(app_for_wezterm_check, state_for_wezterm_check).await;
        });
    }

    let state_for_cleanup = state;
    tauri::async_runtime::spawn(async move {
        stale_cleanup_loop(state_for_cleanup).await;
    });
}

async fn process_ops(
    mut op_rx: mpsc::UnboundedReceiver<Vec<ihstay_core::types::Op>>,
    app: AppHandle,
    state: SharedState,
) {
    while let Some(ops) = op_rx.recv().await {
        let mut s = state.lock().unwrap();
        let count_before = s.store.len();

        for op in ops {
            s.store.apply(op);
        }

        let count_after = s.store.len();
        let entries = s.entries();

        let action = if count_after > count_before {
            s.visibility.handle(VisibilityEvent::EntryAdded {
                board_count: count_after,
            })
        } else if count_after < count_before {
            s.visibility.handle(VisibilityEvent::EntryRemoved {
                board_count: count_after,
            })
        } else {
            VisibilityAction::None
        };

        drop(s);

        let _ = app.emit("entries-updated", &entries);
        let _ = app.emit("badge-count", count_after);
        apply_visibility_action(&app, &action);
    }
}

async fn reaper_loop(_app: AppHandle, state: SharedState) {
    let proc_table = RealProcessTable;
    let session_files = RealSessionFiles::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let entries = {
            let s = state.lock().unwrap();
            s.entries()
        };

        let stale_ops = reaper::sweep(&entries, &proc_table, &session_files);

        if !stale_ops.is_empty() {
            let board_file = board_path();
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&board_file)
            {
                use std::io::Write;
                for op in &stale_ops {
                    if let Ok(line) = serde_json::to_string(op) {
                        let _ = writeln!(file, "{}", line);
                    }
                }
            }
        }
    }
}

async fn visibility_tick_loop(app: AppHandle, state: SharedState) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let action = {
            let mut s = state.lock().unwrap();
            s.visibility.handle(VisibilityEvent::Tick)
        };

        apply_visibility_action(&app, &action);
    }
}

/// Polls every 5 s to see whether the WezTerm processes that were running
/// when the WSLENV-stale warning fired have all exited. When they have,
/// we clear the warning and emit `wezterm-stale-warning-cleared` so the
/// HUD hides the banner without the user having to click the X.
///
/// Heuristic: catches the common case ("user restarted WezTerm") but
/// not the launcher-with-stale-env case (e.g. CmdPal still holding old
/// `WSLENV`) — for that the banner stays up because new wezterm-gui
/// instances inherit the launcher's stale env. The user's manual
/// dismiss covers the remainder.
#[cfg(target_os = "windows")]
async fn wezterm_stale_check_loop(app: AppHandle, state: SharedState) {
    use crate::wsl_env_setup;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let pids = {
            let s = state.lock().unwrap();
            if !s.wezterm_stale_warning || s.stale_wezterm_pids.is_empty() {
                continue;
            }
            s.stale_wezterm_pids.clone()
        };

        if wsl_env_setup::any_pid_alive(&pids) {
            continue;
        }

        {
            let mut s = state.lock().unwrap();
            s.wezterm_stale_warning = false;
            s.stale_wezterm_pids.clear();
        }
        tracing::info!(
            ?pids,
            "all stale wezterm-gui PIDs exited; auto-dismissing WSLENV warning"
        );
        let _ = app.emit("wezterm-stale-warning-cleared", ());
    }
}

/// Periodically emit `clear` ops for stale entries older than [`STALE_TTL`].
///
/// Rationale: a stale entry is only cleared by a matching `UserPromptSubmit`
/// op against the same `session_id`. If the user never resumes that session
/// (e.g. they moved on to a different terminal / session), the entry is
/// orphaned. We don't want stale entries accumulating forever, so this loop
/// writes synthetic `clear` ops through the normal board.jsonl pipeline —
/// the watcher picks them up, store removes the entries, HUD updates.
async fn stale_cleanup_loop(state: SharedState) {
    let board_file = board_path();

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(STALE_CLEANUP_INTERVAL_SECS)).await;

        let now = Utc::now();
        let to_clear: Vec<String> = {
            let s = state.lock().unwrap();
            s.entries()
                .iter()
                .filter(|entry| {
                    entry.state == EntryState::Stale
                        && entry
                            .stale_since
                            .map(|ts| now.signed_duration_since(ts) > STALE_TTL)
                            .unwrap_or(false)
                })
                .map(|e| e.session_id.clone())
                .collect()
        };

        if to_clear.is_empty() {
            continue;
        }

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&board_file)
        {
            Ok(mut file) => {
                use std::io::Write;
                for sid in &to_clear {
                    let op = Op::Clear {
                        ts: now,
                        session_id: sid.clone(),
                        reason: "stale_expired".to_string(),
                    };
                    if let Ok(line) = serde_json::to_string(&op) {
                        let _ = writeln!(file, "{}", line);
                    }
                }
                tracing::info!(count = to_clear.len(), "emitted stale-expired clear ops");
            }
            Err(e) => {
                tracing::warn!(error = %e, "stale cleanup: failed to open board file");
            }
        }
    }
}

fn apply_visibility_action(app: &AppHandle, action: &VisibilityAction) {
    match action {
        VisibilityAction::ShowHud => {
            if let Some(window) = app.get_webview_window("hud") {
                crate::hud_show::show_without_activation(&window);
            }
        }
        VisibilityAction::HideHud => {
            if let Some(window) = app.get_webview_window("hud") {
                let _ = window.hide();
            }
        }
        VisibilityAction::UpdateBadge { count } => {
            let _ = app.emit("badge-count", count);
        }
        VisibilityAction::None => {}
    }
}
