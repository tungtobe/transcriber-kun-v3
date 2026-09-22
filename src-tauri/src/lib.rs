//! trans-kun: modular monolith theo tính năng, một tiến trình duy nhất.
//! Bốn tầng, phụ thuộc chỉ đi xuống: `ipc` → feature → hạ tầng → `core`
//! (xem Architecture Spine, AD-1).

pub mod ads;
pub mod audio;
pub mod core;
pub mod db;
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
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let state = ipc::boot::boot(app.handle());
            app.manage(state);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
