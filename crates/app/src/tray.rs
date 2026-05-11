use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager,
};

pub fn setup(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let open_item = MenuItemBuilder::with_id("open", "Open").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&open_item, &settings_item, &quit_item])
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => {
                let state: tauri::State<crate::state::SharedState> = app.state();
                let mut s = state.lock().unwrap();
                let entries = s.entries();
                let action =
                    s.visibility
                        .handle(ihstay_core::visibility::VisibilityEvent::ManualOpen {
                            board_count: entries.len(),
                        });
                drop(s);

                if action == ihstay_core::visibility::VisibilityAction::ShowHud {
                    if let Some(window) = app.get_webview_window("hud") {
                        crate::hud_show::show_without_activation(&window);
                        let _ = tauri::Emitter::emit(app, "entries-updated", &entries);
                    }
                }
            }
            "settings" => {
                if let Err(e) = crate::commands::open_settings_window(app) {
                    tracing::error!(error = %e, "failed to open settings from tray");
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let state: tauri::State<crate::state::SharedState> = app.state();
                let mut s = state.lock().unwrap();
                let entries = s.entries();
                let action =
                    s.visibility
                        .handle(ihstay_core::visibility::VisibilityEvent::ManualOpen {
                            board_count: entries.len(),
                        });
                drop(s);

                if action == ihstay_core::visibility::VisibilityAction::ShowHud {
                    if let Some(window) = app.get_webview_window("hud") {
                        crate::hud_show::show_without_activation(&window);
                        let _ = tauri::Emitter::emit(app, "entries-updated", &entries);
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
