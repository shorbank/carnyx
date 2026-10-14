use tauri::Manager;

mod chat_state;
pub mod clock;
mod commands;
pub mod identity;
pub mod incoming_message;
pub mod message;
pub mod message_service;
mod migrations;
mod peer;
mod retention;
mod sqlite_identity;
mod sqlite_peer;
pub mod sqlite_repository;
mod storage_error;

#[cfg(target_os = "macos")]
mod startup_lock;

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

    let builder = builder.setup(move |app| {
        #[cfg(target_os = "macos")]
        startup_lock.finish()?;

        let database_path = app.path().app_data_dir()?.join("messages.sqlite");
        let state =
            tauri::async_runtime::block_on(tauri::async_runtime::spawn_blocking(move || {
                chat_state::initialize(database_path)
            }))?
            .map_err(|error| error as Box<dyn std::error::Error>)?;
        app.manage(state.clone());
        app.manage(peer::commands::PeerApp::new(
            app.handle().clone(),
            state.clone(),
        ));
        app.manage(retention::RetentionTask::start(app.handle().clone(), state));
        Ok(())
    });

    builder
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_messages,
            commands::send_message,
            peer::commands::get_peer_setup,
            peer::commands::create_connection_code,
            peer::commands::start_peer_session,
            peer::commands::reconnect_peer,
            peer::commands::disconnect_peer,
            peer::commands::get_peer_state,
            peer::commands::send_peer_message,
            peer::commands::list_peer_messages
        ])
        .build(context)
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Some(peer) = app.try_state::<peer::commands::PeerApp>() {
                    tauri::async_runtime::block_on(peer.coordinator.disconnect());
                }
                if let Some(task) = app.try_state::<retention::RetentionTask>() {
                    task.stop();
                }
            }
        });
}
