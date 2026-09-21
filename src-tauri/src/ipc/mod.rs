//! Adapter UI (AD-1): command/Channel tauri-specta; boot, đóng cửa sổ; nơi
//! duy nhất điều phối chéo feature. Story 1.1 chỉ dựng một command mẫu
//! (`app_version`) và nguồn duy nhất sinh `src/lib/bindings.ts`.

use tauri_specta::{collect_commands, Builder};

#[cfg(test)]
mod spike_channel;

/// Trả về version của app. Nguồn duy nhất là `Cargo.toml`
/// (`CARGO_PKG_VERSION`, đọc lúc biên dịch) — không có nơi thứ hai giữ version.
#[tauri::command]
#[specta::specta]
fn app_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Danh sách command production — nguồn duy nhất, dùng chung cho `lib.rs`
/// (đăng ký `invoke_handler` thật) và test `export_bindings` (sinh
/// `src/lib/bindings.ts`). Không đăng ký command spike ở đây.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![app_version])
}

#[cfg(test)]
mod tests {
    use super::*;
    use specta_typescript::Typescript;

    /// Nguồn duy nhất sinh `src/lib/bindings.ts`; CI chạy lại test này rồi
    /// `git diff --exit-code` để chặn binding lệch bản sinh.
    #[test]
    fn export_bindings() {
        specta_builder()
            .export(Typescript::default(), "../src/lib/bindings.ts")
            .expect("failed to export typescript bindings");
    }
}
