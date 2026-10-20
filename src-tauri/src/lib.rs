mod api;
mod auth;
mod installer;
mod manifest;
mod running;
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

/// Tray menu: open the hub, then Open / Hide / Close for every installed tool, then Quit.
fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    use tauri::menu::{PredefinedMenuItem, Submenu};
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", "Open Toolbay", true, None::<&str>)?)?;
    let lib = store::library(app);
    if !lib.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        let running = app.state::<running::RunState>().tools.lock().unwrap().clone();
        for t in lib {
            let on = running.contains_key(&t.slug);
            let label = if on { format!("{}  ·  running", t.name) } else { t.name.clone() };
            let sub = Submenu::with_items(
                app,
                label,
                true,
                &[
                    &MenuItem::with_id(app, format!("tool:open:{}", t.slug), "Open", true, None::<&str>)?,
                    &MenuItem::with_id(app, format!("tool:hide:{}", t.slug), "Hide", on, None::<&str>)?,
                    &MenuItem::with_id(app, format!("tool:close:{}", t.slug), "Close", on, None::<&str>)?,
                ],
            )?;
            menu.append(&sub)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Toolbay", true, None::<&str>)?)?;
    Ok(menu)
}

pub fn rebuild_tray(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("main"), tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("Toolbay")
        .menu(&tray_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            id => running::handle_tray(app, id),
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

    // Single instance must be registered first: a second launch (a toolbay:// link
    // or a desktop shortcut) forwards its arguments here and exits.
    #[cfg(desktop)]
    {
        // A link can also arrive through on_open_url below; handle_deep_link drops repeats.
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let links: Vec<&String> = argv.iter().filter(|a| a.starts_with("toolbay://")).collect();
            if links.is_empty() {
                show_main(app);
            }
            for link in links {
                auth::handle_deep_link(app, link);
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(auth::AuthState::default())
        .manage(running::RunState::default())
        .manage(installer::Busy::default())
        .setup(|app| {
            let handle = app.handle().clone();

            // On Windows/Linux in dev the scheme is registered at runtime; the
            // installers register it for release builds.
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

            // The window starts hidden so a desktop shortcut (toolbay://launch/<slug>)
            // can open its tool without flashing the hub.
            let mut start_links: Vec<String> = std::env::args()
                .filter(|a| a.starts_with("toolbay://"))
                .chain(app.deep_link().get_current().ok().flatten().unwrap_or_default().into_iter().map(|u| u.to_string()))
                .collect();
            start_links.sort();
            start_links.dedup_by(|a, b| a.trim_end_matches('/') == b.trim_end_matches('/'));
            if !start_links.iter().any(|l| auth::is_launch_link(l)) {
                show_main(&handle);
            }
            for link in &start_links {
                auth::handle_deep_link(&handle, link);
            }

            let _ = installer::APP.set(handle.clone());
            build_tray(&handle)?;
            running::start(&handle);
            installer::start_auto_update(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Toolbay in the tray so tools can update quietly.
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
            installer::tool_resolve,
            installer::tool_install,
            installer::tool_launch,
            running::tool_running,
            running::tool_open,
            running::tool_hide,
            running::tool_close,
            installer::tool_uninstall,
            installer::tool_shortcut,
            installer::tool_reveal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Toolbay");
}
