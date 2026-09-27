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
use tauri_specta::Event;

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
        // Story 2.4 Design Notes (đổi ở story 3.5): "Đóng app: luôn
        // `prevent_close`, hỏi registry async" — the OS close request always
        // arrives synchronously, but whether a Job is running is only
        // knowable by asking the registry actor, which is async;
        // `prevent_close` buys the time for that async check. Story 3.5 spec
        // Code Map: sự kiện đóng cửa sổ giờ LUÔN đi qua frontend (kể cả khi
        // không có Job) để flush ghi chú trước khi thoát -- Rust không còn tự
        // `exit(0)` ở nhánh rảnh, chỉ luôn emit `CloseRequested { busy }` và
        // để `appStore`/`CloseConfirm` quyết định (flush rồi gọi
        // `app_close_confirm`, hoặc mở dialog Job hiện có khi `busy`).
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let window = window.clone();
                tauri::async_runtime::spawn(async move {
                    let state = window.state::<ipc::boot::AppState>();
                    // P0 review fix: when the registry actor is unreachable
                    // (`snapshot()` returns `Err`), fail closed -- treat it as
                    // busy (spec Boundaries Always: "khi `jobs.snapshot()`
                    // trả `Err`, coi là bận").
                    let busy = match &state.jobs {
                        Ok(jobs) => jobs
                            .snapshot()
                            .await
                            .map(|jobs| !jobs.is_empty())
                            .unwrap_or(true),
                        Err(_) => false,
                    };
                    if let Err(err) = (ipc::CloseRequested { busy }).emit(&window) {
                        tracing::warn!(error = %err, "phát event CloseRequested thất bại");
                    }
                });
            }
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
