//! Spike cho ADR 0001: kiểm tauri-specta rc.25 sinh `Channel<T>` typed đúng và
//! runtime gửi/nhận đúng thứ tự `seq` (AD-3 dùng `Channel` cho snapshot + delta).
//!
//! Toàn bộ module chỉ tồn tại trong `cfg(test)`: builder ở đây tách biệt hoàn
//! toàn khỏi `ipc::specta_builder()` dùng cho production, command
//! `spike_stream` không được đăng ký vào `lib.rs` và không xuất ra
//! `src/lib/bindings.ts` (spec Never). Kết luận đầy đủ nằm ở
//! `docs/adr/0001-tauri-specta-channel-typed.md`.
#![cfg(test)]

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri_specta::{collect_commands, Builder};

// `seq` là `u32` chứ không phải `u64`: specta-typescript mặc định cấm xuất
// usize/isize/u64/i64/u128/i128 sang TS ("BigInt forbidden") vì mất chính xác
// qua JSON. Phát hiện này từ spike áp dụng cho mọi field `seq` theo AD-3 —
// xem ADR 0001 mục "Bằng chứng".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct SpikeEvent {
    seq: u32,
    label: String,
}

/// Command chỉ tồn tại trong spike: gửi `count` `SpikeEvent` theo thứ tự tăng
/// dần của `seq` qua `Channel<SpikeEvent>`.
#[tauri::command]
#[specta::specta]
fn spike_stream(on_event: Channel<SpikeEvent>, count: u32) -> Result<(), String> {
    for seq in 0..count {
        on_event
            .send(SpikeEvent {
                seq,
                label: format!("chunk-{seq}"),
            })
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

/// Builder riêng của spike — không dùng chung với builder production.
fn spike_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![spike_stream])
}

#[test]
fn export_renders_typed_channel() {
    use specta_typescript::Typescript;
    use std::fs;

    // rc.25 chỉ có `Builder::export(lang, path)` (không có `export_str`), nên
    // xuất ra một file tạm riêng của spike (không phải `src/lib/bindings.ts`)
    // rồi đọc lại để kiểm nội dung — builder này tách biệt hoàn toàn khỏi
    // `ipc::specta_builder()` production.
    let path = std::env::temp_dir().join(format!(
        "trans-kun-spike-bindings-{}.ts",
        std::process::id()
    ));

    spike_builder()
        .export(Typescript::default(), &path)
        .expect("failed to render typescript for spike builder");

    let ts = fs::read_to_string(&path).expect("failed to read spike export output");
    let _ = fs::remove_file(&path);

    assert!(
        ts.contains("Channel<SpikeEvent>"),
        "kỳ vọng thấy `Channel<SpikeEvent>` typed trong TS sinh ra, thực tế:\n{ts}"
    );
}

#[test]
fn channel_delivers_all_events_in_order() {
    // Dựng thẳng một `tauri::ipc::Channel<SpikeEvent>` thật (không qua mock
    // webview/app) — đây là đúng con đường runtime mà lệnh IPC dùng để
    // serialize (`IpcResponse`) và phát (`on_message`) dữ liệu, nên vẫn kiểm
    // được hành vi runtime thật của `Channel<T>` typed mà không cần dựng một
    // WebView giả (xem ADR 0001 về lý do không đi hết qua `tauri::test`).
    let received: Arc<Mutex<Vec<SpikeEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let received_for_cb = received.clone();

    let channel = Channel::new(move |body| {
        let event: SpikeEvent = match body {
            InvokeResponseBody::Json(json) => {
                serde_json::from_str(&json).expect("payload phải giải mã được thành SpikeEvent")
            }
            InvokeResponseBody::Raw(_) => {
                panic!("SpikeEvent phải serialize thành JSON, không phải raw bytes")
            }
        };
        received_for_cb.lock().unwrap().push(event);
        Ok(())
    });

    const COUNT: u32 = 5;
    spike_stream(channel, COUNT).expect("spike_stream không được lỗi");

    let events = received.lock().unwrap();
    assert_eq!(events.len(), COUNT as usize, "phải nhận đủ số event đã gửi");
    let seqs: Vec<u32> = events.iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        (0..COUNT).collect::<Vec<_>>(),
        "seq phải tăng dần đúng thứ tự đã gửi, không được thiếu hay đảo"
    );
}
