//! trans-kun: modular monolith theo tính năng, một tiến trình duy nhất.
//! Bốn tầng, phụ thuộc chỉ đi xuống: `ipc` → feature → hạ tầng → `core`
//! (xem Architecture Spine, AD-1).

pub mod ads;
pub mod audio;
pub mod consent;
pub mod core;
pub mod db;
pub mod diagnostics;
pub mod gemini;
pub mod ipc;
pub mod library;
pub mod live;
pub mod media;
pub mod memo;
pub mod remote;
pub mod secrets;
pub mod settings;
pub mod transcribe;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = ipc::specta_builder();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let state = ipc::boot::boot(app.handle());
            app.manage(state);
            Ok(())
        })
        // Native window close and app ExitRequested (including Cmd+Q) share
        // the same async coordinator. Prevent each synchronous request first
        // so notes and Live metadata can be flushed before exit.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    ipc::close::request_close(&app).await;
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::ExitRequested { api, .. } => {
                    let state = app_handle.state::<ipc::boot::AppState>();
                    if ipc::close::should_prevent_exit(ipc::close::is_exit_authorized(&state)) {
                        api.prevent_exit();
                        let app = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            ipc::close::request_close(&app).await;
                        });
                    }
                }
                // SIGKILL and OS shutdown do not depend on this callback;
                // boot recovery uses the durable WAV checkpoints.
                tauri::RunEvent::Exit => {
                    let state = app_handle.state::<ipc::boot::AppState>();
                    // Last-resort ducking restore (marker-guarded, no-op when
                    // Live already restored it on Stop).
                    if let Ok(data_dir) = &state.data_dir {
                        ipc::boot::restore_ducked_volume(
                            &mut *audio::output_volume::platform_volume(),
                            data_dir,
                        );
                    }
                    if let Ok(db) = &state.db {
                        if let Err(err) = crate::diagnostics::mark_clean_shutdown(db) {
                            tracing::warn!(error = %err, "could not mark clean shutdown");
                        }
                    }
                }
                _ => {}
            }
        });
}
