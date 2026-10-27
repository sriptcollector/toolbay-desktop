//! The Dock icon (macOS) and taskbar icon (Windows) follow light/dark mode while the app runs.
//! The webview calls `set_app_icon` whenever its theme changes. When the app isn't running,
//! the Dock shows the bundle's own icon (the dark one).
use tauri::AppHandle;

static DARK: &[u8] = include_bytes!("../icons/icon.png");
static LIGHT: &[u8] = include_bytes!("../icons/icon-light.png");

#[tauri::command]
pub fn set_app_icon(app: AppHandle, dark: bool) {
    let bytes = if dark { DARK } else { LIGHT };
    #[cfg(target_os = "macos")]
    {
        let _ = app.run_on_main_thread(move || {
            use objc2::{AllocAnyThread, MainThreadMarker};
            use objc2_app_kit::{NSApplication, NSImage};
            use objc2_foundation::NSData;
            let Some(mtm) = MainThreadMarker::new() else { return };
            let data = NSData::with_bytes(bytes);
            #[allow(unused_unsafe)]
            unsafe {
                if let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) {
                    NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img));
                }
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::Manager;
        if let Ok(img) = tauri::image::Image::from_bytes(bytes) {
            for w in app.webview_windows().values() {
                let _ = w.set_icon(img.clone());
            }
        }
    }
}
