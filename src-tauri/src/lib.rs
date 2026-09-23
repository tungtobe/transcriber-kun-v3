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
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Marker "thoát sạch" (spec Always: "Crash"): chỉ đặt `true` khi
            // tiến trình thực sự đang thoát — `note_boot` (chạy lúc khởi
            // động lần sau) coi mọi trường hợp khác (kill -9, mất điện,
            // panic không unwind) là thoát không sạch và tăng bộ đếm crash.
            if let tauri::RunEvent::Exit = event {
                let state = app_handle.state::<ipc::boot::AppState>();
                if let Ok(db) = &state.db {
                    if let Err(err) = crate::diagnostics::mark_clean_shutdown(db) {
                        tracing::warn!(error = %err, "không ghi được marker thoát sạch");
                    }
                }
            }
        });
}
