//! Template memo (story 3.6, spec Approach): seed lười theo locale, validate
//! tên/prompt, CRUD mẫu người dùng, khôi phục mặc định. Chủ ghi duy nhất cho
//! `memo_templates` ngoài `db/repo/memo_templates.rs` -- `ipc::` gọi qua đây,
//! không gọi thẳng `repo::memo_templates`.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::error::{AppError, Code};
use crate::core::id::MemoTemplateId;
use crate::db::repo;
use crate::db::Db;
use crate::memo::defaults;

/// Giới hạn tên mẫu, đếm theo Unicode scalar (spec Boundaries Always: "tên
/// trim 1–100 ký tự").
pub const MAX_NAME_SCALARS: usize = 100;

/// Giới hạn prompt, đếm theo Unicode scalar (spec Boundaries Always: "prompt
/// ≤ 20 000 ký tự").
pub const MAX_PROMPT_SCALARS: usize = 20_000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Một Template memo qua IPC (spec Code Map: `MemoTemplate { id, name,
/// prompt, isDefault, locale, defaultKey }`). `locale`/`defaultKey` là `None`
/// cho mẫu người dùng.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoTemplate {
    pub id: MemoTemplateId,
    pub name: String,
    pub prompt: String,
    pub is_default: bool,
    pub locale: Option<String>,
    pub default_key: Option<String>,
}

fn to_public(row: repo::memo_templates::MemoTemplateRow) -> MemoTemplate {
    MemoTemplate {
        id: row.id,
        name: row.name,
        prompt: row.prompt,
        is_default: row.is_default,
        locale: row.locale,
        default_key: row.default_key,
    }
}

/// `locale` chỉ nhận `vi`/`en`/`ja` -- giá trị khác bị từ chối (spec Code
/// Map: "Locale nhận chuỗi `vi|en|ja`, giá trị khác → lỗi `Request`").
fn validate_locale(locale: &str) -> Result<(), AppError> {
    if defaults::defaults_for_locale(locale).is_none() {
        return Err(AppError::new(
            Code::Request,
            format!("Locale không được hỗ trợ: {locale}"),
        ));
    }
    Ok(())
}

/// Tên: trim rồi giới hạn 1–100 ký tự Unicode scalar (spec Boundaries
/// Always). Rỗng sau trim bị từ chối cùng lỗi với "> 100 ký tự" -- cả hai
/// đều `Code::Request`.
fn validate_name(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::new(Code::Request, "Tên mẫu không được để trống"));
    }
    if trimmed.chars().count() > MAX_NAME_SCALARS {
        return Err(AppError::new(
            Code::Request,
            format!("Tên mẫu vượt quá {MAX_NAME_SCALARS} ký tự"),
        ));
    }
    Ok(trimmed.to_string())
}

/// Prompt: ≤ 20 000 ký tự Unicode scalar và phải chứa `{transcript}`;
/// `{notes}` tuỳ chọn (spec Boundaries Always). Dùng chung ở Rust lẫn để tài
/// liệu hoá đúng quy tắc UI phải lặp lại (dòng kiểm tra trực tiếp).
pub fn validate_prompt(raw: &str) -> Result<(), AppError> {
    if raw.chars().count() > MAX_PROMPT_SCALARS {
        return Err(AppError::new(
            Code::Request,
            format!("Prompt vượt quá {MAX_PROMPT_SCALARS} ký tự"),
        ));
    }
    if !raw.contains("{transcript}") {
        return Err(AppError::new(
            Code::Request,
            "Prompt phải chứa {transcript}",
        ));
    }
    Ok(())
}

/// Đảm bảo hai mẫu mặc định của `locale` tồn tại -- gọi trong cùng
/// transaction/connection với `list` mỗi lần (spec Design Notes: "Seed lười
/// trong `memo_templates_list(locale)` (thay vì lúc boot) vì chỉ frontend
/// biết locale UI thực tế"). `locale` phải đã qua [`validate_locale`].
fn ensure_seeded(conn: &rusqlite::Connection, locale: &str, now: i64) -> Result<(), AppError> {
    let defs = defaults::defaults_for_locale(locale).expect("locale đã được validate trước đó");
    for def in defs {
        repo::memo_templates::ensure_default(
            conn,
            MemoTemplateId::new(),
            def.name,
            def.prompt,
            locale,
            def.default_key,
            now,
        )?;
    }
    Ok(())
}

/// Hai mẫu mặc định của `locale` + mọi mẫu người dùng (spec Boundaries
/// Always). Seed lười trước khi đọc trong cùng lần gọi `with_connection` --
/// không có cửa sổ đua giữa seed và đọc vì cả hai chạy trên cùng connection,
/// tuần tự (`Db::with_connection` khoá cả tiến trình).
pub fn list(db: &Db, locale: &str) -> Result<Vec<MemoTemplate>, AppError> {
    validate_locale(locale)?;
    let now = now_ms();
    db.with_connection(|conn| {
        ensure_seeded(conn, locale, now)?;
        Ok(repo::memo_templates::list(conn, locale)?
            .into_iter()
            .map(to_public)
            .collect())
    })
}

/// Tạo một mẫu người dùng mới -- validate tên/prompt trước khi chạm DB (spec
/// I/O Matrix: một tên/prompt không hợp lệ không bao giờ tạo giao dịch DB).
pub fn create(db: &Db, name: String, prompt: String) -> Result<MemoTemplate, AppError> {
    let name = validate_name(&name)?;
    validate_prompt(&prompt)?;
    let id = MemoTemplateId::new();
    let now = now_ms();
    db.with_connection(|conn| {
        repo::memo_templates::insert(conn, id, &name, &prompt, now)?;
        Ok(repo::memo_templates::get(conn, id)?
            .map(to_public)
            .expect("vừa insert xong phải đọc lại được"))
    })
}

/// Sửa tên/prompt của một mẫu đã có, kể cả mẫu mặc định (spec Boundaries
/// Always: "Mẫu mặc định sửa được (tên/prompt) nhưng không xoá được").
pub fn update(
    db: &Db,
    id: MemoTemplateId,
    name: String,
    prompt: String,
) -> Result<MemoTemplate, AppError> {
    let name = validate_name(&name)?;
    validate_prompt(&prompt)?;
    let now = now_ms();
    db.with_connection(|conn| {
        let affected = repo::memo_templates::update_name_prompt(conn, id, &name, &prompt, now)?;
        if affected == 0 {
            return Err(AppError::new(Code::Request, "Mẫu không tồn tại"));
        }
        Ok(repo::memo_templates::get(conn, id)?
            .map(to_public)
            .expect("vừa update xong phải đọc lại được"))
    })
}

/// Xoá một mẫu người dùng -- từ chối nếu là mẫu mặc định (spec Boundaries
/// Always: "không xoá được (Rust từ chối, UI vô hiệu nút)"), idempotent nếu
/// `id` không tồn tại (cùng quy ước `library::tags::delete_tag`).
pub fn delete(db: &Db, id: MemoTemplateId) -> Result<(), AppError> {
    db.with_connection(|conn| match repo::memo_templates::get(conn, id)? {
        None => Ok(()),
        Some(row) if row.is_default => {
            Err(AppError::new(Code::Request, "Không thể xoá mẫu mặc định"))
        }
        Some(_) => {
            repo::memo_templates::delete_user_template(conn, id)?;
            Ok(())
        }
    })
}

/// Ghi lại tên/prompt gốc cho hai mẫu mặc định của `locale` (tạo lại nếu
/// thiếu) -- không đụng mẫu người dùng hay locale khác (spec Boundaries
/// Always/Never). Trả danh sách mới nhất của `locale` đó để frontend cập
/// nhật ngay không cần gọi `list` riêng.
pub fn restore_defaults(db: &Db, locale: &str) -> Result<Vec<MemoTemplate>, AppError> {
    validate_locale(locale)?;
    let now = now_ms();
    db.with_connection(|conn| {
        let defs = defaults::defaults_for_locale(locale).expect("locale đã được validate trước đó");
        for def in defs {
            repo::memo_templates::restore_default(
                conn,
                MemoTemplateId::new(),
                def.name,
                def.prompt,
                locale,
                def.default_key,
                now,
            )?;
        }
        Ok(repo::memo_templates::list(conn, locale)?
            .into_iter()
            .map(to_public)
            .collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_db() -> (tempfile::TempDir, Db) {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        (dir, db)
    }

    #[test]
    fn list_seeds_exactly_two_defaults_on_first_call() {
        let (_dir, db) = open_db();
        let rows = list(&db, "vi").unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row.is_default));
        assert_eq!(rows[0].default_key.as_deref(), Some("meeting-minutes"));
        assert_eq!(rows[1].default_key.as_deref(), Some("bilingual-ja-vi"));
    }

    /// Spec I/O Matrix "Mở lại nhiều lần": gọi `list(vi)` 3 lần vẫn đúng 2
    /// mẫu mặc định.
    #[test]
    fn calling_list_repeatedly_never_duplicates_defaults() {
        let (_dir, db) = open_db();
        list(&db, "vi").unwrap();
        list(&db, "vi").unwrap();
        let rows = list(&db, "vi").unwrap();
        assert_eq!(rows.len(), 2);
    }

    /// Spec I/O Matrix "Đổi locale": mẫu vi đã sửa giữ nguyên trong DB, không
    /// hiện ở list ja.
    #[test]
    fn editing_a_default_in_one_locale_does_not_leak_into_another_locales_list() {
        let (_dir, db) = open_db();
        let vi_rows = list(&db, "vi").unwrap();
        update(
            &db,
            vi_rows[0].id,
            "Tên đã sửa".to_string(),
            "{transcript} sửa".to_string(),
        )
        .unwrap();

        let ja_rows = list(&db, "ja").unwrap();
        assert_eq!(ja_rows.len(), 2);
        assert!(ja_rows.iter().all(|row| row.name != "Tên đã sửa"));

        let vi_rows_again = list(&db, "vi").unwrap();
        assert_eq!(
            vi_rows_again[0].name, "Tên đã sửa",
            "sửa mẫu vi phải giữ nguyên trong DB"
        );
    }

    /// Mẫu người dùng không gắn locale nên hiện ở mọi locale, không nhân bản.
    #[test]
    fn a_user_template_appears_in_every_locales_list() {
        let (_dir, db) = open_db();
        list(&db, "vi").unwrap();
        let created = create(&db, "Của tôi".to_string(), "{transcript}".to_string()).unwrap();

        let vi_rows = list(&db, "vi").unwrap();
        let ja_rows = list(&db, "ja").unwrap();
        assert!(vi_rows.iter().any(|row| row.id == created.id));
        assert!(ja_rows.iter().any(|row| row.id == created.id));
    }

    #[test]
    fn create_rejects_blank_name_and_missing_transcript_placeholder() {
        let (_dir, db) = open_db();
        let err = create(&db, "   ".to_string(), "{transcript}".to_string()).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);

        let too_long_name = "a".repeat(MAX_NAME_SCALARS + 1);
        let err = create(&db, too_long_name, "{transcript}".to_string()).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);

        let err = create(&db, "tên hợp lệ".to_string(), "Tóm tắt {notes}".to_string()).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);

        let too_long_prompt = format!("{{transcript}}{}", "a".repeat(MAX_PROMPT_SCALARS));
        let err = create(&db, "tên hợp lệ".to_string(), too_long_prompt).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
    }

    #[test]
    fn create_trims_the_name_and_round_trips_via_list() {
        let (_dir, db) = open_db();
        let created = create(&db, "  Tên mẫu  ".to_string(), "{transcript}".to_string()).unwrap();
        assert_eq!(created.name, "Tên mẫu");
        assert!(!created.is_default);
        assert_eq!(created.locale, None);

        let rows = list(&db, "vi").unwrap();
        assert!(rows
            .iter()
            .any(|row| row.id == created.id && row.name == "Tên mẫu"));
    }

    #[test]
    fn update_edits_a_default_template_in_place() {
        let (_dir, db) = open_db();
        let rows = list(&db, "en").unwrap();
        let target = rows[0].id;
        let updated = update(
            &db,
            target,
            "Custom name".to_string(),
            "{transcript} v2".to_string(),
        )
        .unwrap();
        assert_eq!(updated.name, "Custom name");
        assert!(updated.is_default, "vẫn là mẫu mặc định sau khi sửa");
    }

    #[test]
    fn update_returns_request_error_for_an_unknown_id() {
        let (_dir, db) = open_db();
        let err = update(
            &db,
            MemoTemplateId::new(),
            "x".to_string(),
            "{transcript}".to_string(),
        )
        .unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
    }

    /// Spec I/O Matrix "Xoá mặc định": từ chối, không xoá.
    #[test]
    fn delete_rejects_a_default_template_without_removing_it() {
        let (_dir, db) = open_db();
        let rows = list(&db, "vi").unwrap();
        let default_id = rows[0].id;
        let err = delete(&db, default_id).unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
        assert!(list(&db, "vi")
            .unwrap()
            .iter()
            .any(|row| row.id == default_id));
    }

    /// Spec I/O Matrix "Xoá mẫu người dùng": mẫu biến mất.
    #[test]
    fn delete_removes_a_user_template_and_is_idempotent() {
        let (_dir, db) = open_db();
        let created = create(&db, "Của tôi".to_string(), "{transcript}".to_string()).unwrap();
        delete(&db, created.id).unwrap();
        assert!(!list(&db, "vi")
            .unwrap()
            .iter()
            .any(|row| row.id == created.id));
        // Xoá lại lần nữa vẫn `Ok` (idempotent).
        delete(&db, created.id).unwrap();
    }

    /// Spec I/O Matrix "Khôi phục": mẫu vi mặc định đã sửa + 1 mẫu người dùng
    /// -- mặc định vi về gốc; mẫu người dùng còn.
    #[test]
    fn restore_defaults_resets_edited_defaults_but_keeps_user_templates_and_other_locales() {
        let (_dir, db) = open_db();
        let vi_rows = list(&db, "vi").unwrap();
        let original_name = vi_rows[0].name.clone();
        let original_prompt = vi_rows[0].prompt.clone();
        update(
            &db,
            vi_rows[0].id,
            "Đã sửa".to_string(),
            "{transcript} sửa".to_string(),
        )
        .unwrap();
        let user = create(&db, "Của tôi".to_string(), "{transcript}".to_string()).unwrap();

        let ja_rows_before = list(&db, "ja").unwrap();
        let ja_original_name = ja_rows_before[0].name.clone();
        update(
            &db,
            ja_rows_before[0].id,
            "JA đã sửa".to_string(),
            "{transcript} ja".to_string(),
        )
        .unwrap();

        let restored = restore_defaults(&db, "vi").unwrap();
        assert_eq!(restored[0].name, original_name);
        assert_eq!(restored[0].prompt, original_prompt);
        assert_eq!(restored[0].id, vi_rows[0].id, "restore giữ nguyên id");

        // Mẫu người dùng còn nguyên.
        assert!(list(&db, "vi").unwrap().iter().any(|row| row.id == user.id));
        // Locale khác (ja) không bị đụng tới.
        let ja_rows_after = list(&db, "ja").unwrap();
        assert_eq!(ja_rows_after[0].name, "JA đã sửa");
        let _ = ja_original_name;
    }

    /// Spec Boundaries Always: locale khác `vi|en|ja` bị từ chối cho mọi hàm
    /// nhận locale.
    #[test]
    fn unsupported_locale_is_rejected_for_list_and_restore() {
        let (_dir, db) = open_db();
        let err = list(&db, "fr").unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
        let err = restore_defaults(&db, "fr").unwrap_err();
        assert_eq!(err.category, crate::core::error::Category::Model);
    }
}
