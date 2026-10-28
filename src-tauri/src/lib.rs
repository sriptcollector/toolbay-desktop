mod api;
mod auth;
mod firstrun;
mod installer;
mod manifest;
mod running;
mod store;
mod sync;

use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
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
        .plugin(tauri_plugin_dialog::init())
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

            installer::ci_smoke_test(&handle);
            firstrun::check(&handle);
            running::start(&handle);
            sync::start(&handle);
            installer::start_auto_update(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window quits Toolbay; nothing keeps running in the background. Only an
            // install or update that's mid-copy finishes first (stopping it would break the tool).
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().clone();
                if window.label() == "main" && !app.state::<installer::Busy>().0.lock().unwrap().is_empty() {
                    api.prevent_close();
                    let _ = window.hide();
                    tauri::async_runtime::spawn(async move {
                        while !app.state::<installer::Busy>().0.lock().unwrap().is_empty() {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        // Opened again meanwhile (a second launch shows it): keep running.
                        let reopened = app.get_webview_window("main").is_some_and(|w| w.is_visible().unwrap_or(false));
                        if !reopened {
                            app.exit(0);
                        }
                    });
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            api::api_request,
            api::package_upload,
            firstrun::route_take,
            store::library_rename,
            auth::auth_start,
            auth::auth_user,
            auth::auth_sign_out,
            store::settings_get,
            store::settings_set,
            store::library_list,
            installer::tool_resolve,
            installer::tool_install,
            installer::tool_pending_updates,
            installer::tool_update_now,
            installer::tool_launch,
            running::tool_running,
            running::tool_open,
            running::tool_close,
            sync::sync_other_tools,
            installer::tool_uninstall,
            installer::tool_shortcut,
            installer::tool_reveal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Toolbay");
}
