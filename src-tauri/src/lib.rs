mod api;
mod auth;
mod flow;
mod installer;
mod store;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Toolbay", true, None::<&str>)?;
    let flow = MenuItem::with_id(app, "flow", "Toolbay Flow…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &flow, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Toolbay")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "flow" => {
                show_main(app);
                let _ = tauri::Emitter::emit(app, "open-route", serde_json::json!({"kind":"tool","slug":"toolbay-flow"}));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Single instance must be registered first: a second launch (e.g. from a
    // toolbay:// link on Windows) forwards its URL here and exits.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            show_main(app);
            for arg in argv.iter().filter(|a| a.starts_with("toolbay://")) {
                auth::handle_deep_link(app, arg);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| flow::on_shortcut(app, event.state()))
                .build(),
        )
        .manage(auth::AuthState::default())
        .manage(flow::Flow::default())
        .setup(|app| {
            let handle = app.handle().clone();

            // toolbay:// deep links. On Windows/Linux in dev the scheme is
            // registered at runtime; installers register it for release builds.
            #[cfg(any(windows, target_os = "linux"))]
            {
                let _ = app.deep_link().register_all();
            }
            let h = handle.clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    auth::handle_deep_link(&h, url.as_str());
                }
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    auth::handle_deep_link(&handle, url.as_str());
                }
            }

            build_tray(&handle)?;
            flow::restore(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Toolbay in the tray (Flow keeps listening).
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            api::api_request,
            auth::auth_start,
            auth::auth_user,
            auth::auth_sign_out,
            store::settings_get,
            store::settings_set,
            store::library_list,
            installer::tool_install,
            installer::tool_launch,
            installer::tool_reveal,
            installer::tool_uninstall,
            installer::skill_install,
            flow::flow_status,
            flow::flow_enable,
            flow::flow_download_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Toolbay");
}
