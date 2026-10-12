#[cfg(desktop)]
use tauri::Manager;

#[cfg(target_os = "macos")]
mod startup_lock;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let context = tauri::generate_context!();
    #[cfg(target_os = "macos")]
    let startup_lock = startup_lock::StartupLock::acquire(&context.config().identifier)
        .expect("could not acquire the application startup lock");

    let builder = tauri::Builder::default();

    // Register first so duplicate launches exit before a window is created.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(window) = app.get_webview_window("main") {
            if let Err(error) = window.show() {
                eprintln!("Could not show the existing chat window: {error}");
            }
            if let Err(error) = window.unminimize() {
                eprintln!("Could not restore the existing chat window: {error}");
            }
            if let Err(error) = window.set_focus() {
                eprintln!("Could not focus the existing chat window: {error}");
            }
        }
    }));

    #[cfg(target_os = "macos")]
    let builder = builder.setup(move |_| {
        startup_lock.finish()?;
        Ok(())
    });

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(context)
        .expect("error while running tauri application");
}
