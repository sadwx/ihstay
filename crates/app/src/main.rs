#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod hud_show;
mod plugin_install;
mod plugin_watch;
mod services;
mod state;
mod tray;
#[cfg(target_os = "windows")]
mod wsl_env_setup;

use state::{AppState, SharedState};
use std::sync::{Arc, Mutex};

fn main() {
    // CLI subcommand: run sanitize and exit without booting Tauri. Lets users
    // who don't keep the tray app running keep the installed plugin.json
    // tidy after `claude plugin install/update` (cron, launchd, manual).
    if std::env::args().any(|a| a == "--sanitize-manifest") {
        run_sanitize_and_exit();
    }

    init_tracing();

    let shared_state: SharedState = Arc::new(Mutex::new(AppState::new()));

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .manage(shared_state.clone())
        .setup(move |app| {
            // Run as a menu-bar agent on macOS — no Dock icon, no app-switcher
            // entry. The HUD and Settings windows still appear when shown;
            // closing the last window does not exit the app (tray keeps it
            // alive). Must come before any window is shown.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Best-effort: keep the installed plugin.json clean of hook
            // entries for other OSes. Claude Code 2.1.x ignores the
            // `platform` field on hook entries, so without this they show
            // up in `/hooks` and ENOENT on every fire. Non-fatal on error.
            tauri::async_runtime::spawn_blocking(|| {
                let _ = plugin_install::sanitize_installed_plugin_json();
            });

            // Build the HUD window before booting services so the async op pipeline
            // can always find it via get_webview_window("hud"). Without this, ops
            // loaded from a non-empty board.jsonl at startup can race the window
            // creation and silently drop the ShowHud action.
            let _hud_window = tauri::WebviewWindowBuilder::new(
                app,
                "hud",
                tauri::WebviewUrl::App("hud/index.html".into()),
            )
            .title("IHSTAY")
            .inner_size(380.0, 240.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .visible(false)
            .skip_taskbar(true)
            .build()?;

            // Pre-create the Settings window hidden. Creating it here during
            // setup (main thread) avoids race conditions and webview hangs we
            // saw when creating it on-demand from a command handler.
            let settings_window = tauri::WebviewWindowBuilder::new(
                app,
                "settings",
                tauri::WebviewUrl::App("settings/index.html".into()),
            )
            .title("Settings - IHSTAY")
            .inner_size(480.0, 500.0)
            .resizable(true)
            .visible(false)
            .skip_taskbar(true)
            .build()?;

            // Intercept the close button: hide the window instead of destroying
            // it, so we can keep reopening the same window.
            let settings_handle = settings_window.clone();
            settings_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = settings_handle.hide();
                }
            });

            services::boot(app.handle(), shared_state.clone());
            tray::setup(app)?;

            // Auto-configure WSLENV so click-to-focus works for WSL-origin
            // entries without manual `setx` from the user. Runs in a
            // blocking task — touches the registry and may shell out to
            // PowerShell for the broadcast on first run, neither of which
            // should hold up app boot.
            //
            // After the registry write, if WezTerm is already running, its
            // env is now stale: WezTerm captures WSLENV at launch and never
            // re-reads it, so WSL panes inherit the old value and the bash
            // hook can't see WEZTERM_PANE. Surface a one-shot warning to
            // the HUD so the user knows to restart WezTerm.
            #[cfg(target_os = "windows")]
            {
                use tauri::{Emitter, Manager};
                let warning_state = shared_state.clone();
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let status = wsl_env_setup::ensure_wsl_env_tokens();
                    if status != wsl_env_setup::Status::Updated {
                        return;
                    }
                    let pids = wsl_env_setup::wezterm_gui_pids();
                    if pids.is_empty() {
                        return;
                    }
                    tracing::warn!(
                        ?pids,
                        "WSLENV updated while wezterm-gui is running — restart WezTerm \
                         so WSL panes pick up WEZTERM_PANE/u; click-to-focus into WSL \
                         will fall back to spawn-a-new-tab until then"
                    );
                    {
                        let mut s = warning_state.lock().unwrap();
                        s.wezterm_stale_warning = true;
                        s.stale_wezterm_pids = pids;
                    }
                    let _ = app_handle.emit("wezterm-stale-warning", ());
                    if let Some(window) = app_handle.get_webview_window("hud") {
                        crate::hud_show::show_without_activation(&window);
                    }
                });
            }

            tracing::info!("IHSTAY started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_entries,
            commands::focus_entry,
            commands::dismiss_hud,
            commands::dismiss_panel_opened,
            commands::wezterm_stale_warning_active,
            commands::dismiss_wezterm_stale_warning,
            commands::manual_open,
            commands::open_settings,
            commands::hide_settings,
            commands::reset_hud_position,
            commands::get_config,
            commands::apply_config,
            commands::check_hooks_installed,
            commands::install_plugin,
            commands::dismiss_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Configure `tracing` to write to `~/.claude/pending/logs/app.log` instead
/// of stderr. macOS .app bundles launched from Finder / login items have
/// closed or broken stderr, so the default fmt subscriber's first write
/// triggers `__eprint`'s `failed printing to stderr` panic — which aborts
/// the process. We saw this in production via a SIGABRT whose faulting
/// frame was `tracing_subscriber::fmt::Subscriber::event` →
/// `tracing_core::event::Event::dispatch` →
/// `commands::open_settings_window`. Writing to a file removes the stderr
/// dependency entirely. If the file can't be opened (read-only HOME, quota,
/// etc.) fall back to `io::sink` rather than re-introducing the panicking
/// stderr writer.
fn init_tracing() {
    use std::sync::Mutex;
    use tracing_subscriber::EnvFilter;

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ihstay=info"));

    let log_file = dirs_next::home_dir().and_then(|home| {
        let dir = home.join(".claude/pending/logs");
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("app.log"))
            .ok()
    });

    let builder = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_ansi(false);

    match log_file {
        Some(f) => builder.with_writer(Mutex::new(f)).init(),
        None => builder.with_writer(std::io::sink).init(),
    }
}

fn run_sanitize_and_exit() -> ! {
    match plugin_install::sanitize_installed_plugin_json() {
        Ok(0) => {
            eprintln!("plugin.json already clean — no foreign-platform entries.");
            std::process::exit(0);
        }
        Ok(removed) => {
            eprintln!("removed {removed} foreign-platform hook entries from plugin.json.");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("sanitize failed: {e}");
            std::process::exit(1);
        }
    }
}
