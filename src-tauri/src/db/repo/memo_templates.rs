//! SQL cho `memo_templates` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL cho bảng này (spec Boundaries). Validate độ dài,
//! seed lười, và quy tắc "không xoá mặc định" nằm ở `memo::templates`, không
//! ở đây — module này chỉ đọc/ghi thẳng.

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::MemoTemplateId;

#[derive(Debug, Clone, PartialEq)]
pub struct MemoTemplateRow {
    pub id: MemoTemplateId,
    pub name: String,
    pub prompt: String,
    pub is_default: bool,
    pub locale: Option<String>,
    pub default_key: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn parse_id(raw: &str) -> rusqlite::Result<MemoTemplateId> {
    MemoTemplateId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `memo_templates.id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

fn row_to_template(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoTemplateRow> {
    let id: String = row.get(0)?;
    let is_default: i64 = row.get(3)?;
    Ok(MemoTemplateRow {
        id: parse_id(&id)?,
        name: row.get(1)?,
        prompt: row.get(2)?,
        is_default: is_default != 0,
        locale: row.get(4)?,
        default_key: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const SELECT_COLUMNS: &str =
    "id, name, prompt, is_default, locale, default_key, created_at, updated_at";

/// Chèn một mẫu mặc định nếu `(locale, default_key)` chưa tồn tại -- không
/// bao giờ ghi đè mẫu người dùng đã sửa (spec Boundaries Always:
/// "`memo_templates_list(locale)` đảm bảo bộ mặc định của locale đó tồn tại
/// (chèn nếu thiếu theo `(locale, default_key)`, không bao giờ nhân bản,
/// không ghi đè bản mặc định người dùng đã sửa)"). `ON CONFLICT DO NOTHING`
/// làm câu này idempotent kể cả khi gọi đồng thời.
pub fn ensure_default(
    conn: &Connection,
    id: MemoTemplateId,
    name: &str,
    prompt: &str,
    locale: &str,
    default_key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?6) \
         ON CONFLICT (locale, default_key) DO NOTHING",
        params![id.to_string(), name, prompt, locale, default_key, now],
    )?;
    Ok(())
}

/// Ghi lại tên/prompt gốc của một mẫu mặc định, tạo lại nếu thiếu (spec
/// Boundaries Always: "`memo_templates_restore_defaults(locale)` ghi lại
/// tên/prompt gốc cho các mẫu mặc định của locale đó (tạo lại nếu thiếu)").
/// Cùng ràng buộc `UNIQUE(locale, default_key)` với [`ensure_default`] nhưng
/// nhánh `DO UPDATE` thay vì `DO NOTHING` -- dòng đã tồn tại giữ nguyên `id`
/// (không nằm trong `SET`), chỉ tên/prompt/`updated_at` bị ghi đè.
pub fn restore_default(
    conn: &Connection,
    id: MemoTemplateId,
    name: &str,
    prompt: &str,
    locale: &str,
    default_key: &str,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?6, ?6) \
         ON CONFLICT (locale, default_key) DO UPDATE SET \
             name = excluded.name, prompt = excluded.prompt, updated_at = excluded.updated_at",
        params![id.to_string(), name, prompt, locale, default_key, now],
    )?;
    Ok(())
}

/// Hai mẫu mặc định của `locale` + mọi mẫu người dùng (spec Boundaries
/// Always: "mặc định trước theo thứ tự cố định, rồi mẫu người dùng theo
/// `created_at`"). Mẫu người dùng không gắn `locale` (`NULL`) nên hiện ở mọi
/// locale -- chỉ mẫu *mặc định* mới lọc theo `locale`.
pub fn list(conn: &Connection, locale: &str) -> rusqlite::Result<Vec<MemoTemplateRow>> {
    let sql = format!(
        "SELECT {SELECT_COLUMNS} FROM memo_templates \
         WHERE (is_default = 1 AND locale = ?1) OR is_default = 0 \
         ORDER BY is_default DESC, \
             CASE WHEN is_default = 1 THEN \
                 CASE default_key WHEN 'meeting-minutes' THEN 0 WHEN 'bilingual-ja-vi' THEN 1 ELSE 2 END \
             ELSE 0 END, \
             created_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![locale], row_to_template)?;
    rows.collect()
}

/// Đọc một mẫu theo `id` -- dùng để trả lại dòng vừa tạo/sửa qua IPC.
pub fn get(conn: &Connection, id: MemoTemplateId) -> rusqlite::Result<Option<MemoTemplateRow>> {
    let sql = format!("SELECT {SELECT_COLUMNS} FROM memo_templates WHERE id = ?1");
    conn.query_row(&sql, params![id.to_string()], row_to_template)
        .optional()
}

/// Tạo một mẫu người dùng mới -- `is_default = 0`, `locale`/`default_key`
/// đều `NULL` (spec Boundaries Always).
pub fn insert(
    conn: &Connection,
    id: MemoTemplateId,
    name: &str,
    prompt: &str,
    now: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO memo_templates (id, name, prompt, is_default, locale, default_key, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 0, NULL, NULL, ?4, ?4)",
        params![id.to_string(), name, prompt, now],
    )?;
    Ok(())
}

/// Sửa tên/prompt của một mẫu đã có -- áp dụng cho cả mẫu mặc định (sửa được,
/// spec Boundaries Always: "Mẫu mặc định sửa được (tên/prompt) nhưng không
/// xoá được") lẫn mẫu người dùng. `0` dòng đổi nghĩa là `id` không tồn tại.
pub fn update_name_prompt(
    conn: &Connection,
    id: MemoTemplateId,
    name: &str,
    prompt: &str,
    now: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE memo_templates SET name = ?2, prompt = ?3, updated_at = ?4 WHERE id = ?1",
        params![id.to_string(), name, prompt, now],
    )
}

/// Xoá một mẫu *người dùng* -- `AND is_default = 0` là lớp bảo vệ thứ hai
/// (lớp thứ nhất ở `memo::templates::delete`, đọc dòng trước để phân biệt
/// "không tồn tại" (`Ok`, idempotent) với "là mẫu mặc định" (`Err`)): câu SQL
/// này xoá `0` dòng cho cả hai trường hợp, không tự phân biệt được.
pub fn delete_user_template(conn: &Connection, id: MemoTemplateId) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM memo_templates WHERE id = ?1 AND is_default = 0",
        params![id.to_string()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn open_migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn ensure_default_inserts_once_and_ignores_a_second_call() {
        let conn = open_migrated();
        let id = MemoTemplateId::new();
        ensure_default(
            &conn,
            id,
            "Biên bản họp",
            "{transcript}",
            "vi",
            "meeting-minutes",
            0,
        )
        .unwrap();
        // Gọi lại với id khác -- không được tạo dòng thứ hai, không đổi tên.
        ensure_default(
            &conn,
            MemoTemplateId::new(),
            "tên khác",
            "{transcript}",
            "vi",
            "meeting-minutes",
            0,
        )
        .unwrap();

        let rows = list(&conn, "vi").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].name, "Biên bản họp");
    }

    #[test]
    fn ensure_default_does_not_overwrite_a_user_edited_default() {
        let conn = open_migrated();
        let id = MemoTemplateId::new();
        ensure_default(
            &conn,
            id,
            "Biên bản họp",
            "{transcript}",
            "vi",
            "meeting-minutes",
            0,
        )
        .unwrap();
        update_name_prompt(&conn, id, "Tên đã sửa", "{transcript} sửa", 1).unwrap();

        ensure_default(
            &conn,
            MemoTemplateId::new(),
            "Biên bản họp",
            "{transcript}",
            "vi",
            "meeting-minutes",
            2,
        )
        .unwrap();

        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(
            row.name, "Tên đã sửa",
            "seed lười không được ghi đè sửa đổi"
        );
    }

    #[test]
    fn restore_default_overwrites_an_edited_default_and_recreates_a_missing_one() {
        let conn = open_migrated();
        let id = MemoTemplateId::new();
        ensure_default(
            &conn,
            id,
            "Biên bản họp",
            "{transcript}",
            "vi",
            "meeting-minutes",
            0,
        )
        .unwrap();
        update_name_prompt(&conn, id, "Tên đã sửa", "{transcript} sửa", 1).unwrap();

        restore_default(
            &conn,
            id,
            "Biên bản họp",
            "{transcript} gốc",
            "vi",
            "meeting-minutes",
            2,
        )
        .unwrap();
        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.id, id, "giữ nguyên id khi dòng đã tồn tại");
        assert_eq!(row.name, "Biên bản họp");
        assert_eq!(row.prompt, "{transcript} gốc");

        // Thiếu hẳn (chưa từng seed) -- restore phải tạo lại.
        let recreated_id = MemoTemplateId::new();
        restore_default(
            &conn,
            recreated_id,
            "Memo song ngữ",
            "{transcript}{notes}",
            "vi",
            "bilingual-ja-vi",
            3,
        )
        .unwrap();
        let recreated = get(&conn, recreated_id).unwrap().unwrap();
        assert_eq!(recreated.default_key.as_deref(), Some("bilingual-ja-vi"));
    }

    #[test]
    fn list_returns_defaults_of_the_given_locale_plus_every_user_template_ordered() {
        let conn = open_migrated();
        ensure_default(
            &conn,
            MemoTemplateId::new(),
            "Biên bản họp",
            "{transcript}",
            "vi",
            "meeting-minutes",
            0,
        )
        .unwrap();
        ensure_default(
            &conn,
            MemoTemplateId::new(),
            "Memo song ngữ Nhật–Việt",
            "{transcript}",
            "vi",
            "bilingual-ja-vi",
            0,
        )
        .unwrap();
        ensure_default(
            &conn,
            MemoTemplateId::new(),
            "議事録",
            "{transcript}",
            "ja",
            "meeting-minutes",
            0,
        )
        .unwrap();
        let user_id = MemoTemplateId::new();
        insert(&conn, user_id, "Mẫu của tôi", "{transcript}", 10).unwrap();

        let vi_rows = list(&conn, "vi").unwrap();
        assert_eq!(vi_rows.len(), 3, "2 mặc định vi + 1 mẫu người dùng");
        assert_eq!(vi_rows[0].default_key.as_deref(), Some("meeting-minutes"));
        assert_eq!(vi_rows[1].default_key.as_deref(), Some("bilingual-ja-vi"));
        assert_eq!(vi_rows[2].id, user_id, "mẫu người dùng hiện ở mọi locale");

        let ja_rows = list(&conn, "ja").unwrap();
        assert_eq!(
            ja_rows.len(),
            2,
            "chỉ 1 mặc định ja + mẫu người dùng, không lẫn mặc định vi"
        );
        assert_eq!(ja_rows[1].id, user_id);
    }

    #[test]
    fn delete_user_template_never_removes_a_default_row() {
        let conn = open_migrated();
        let default_id = MemoTemplateId::new();
        ensure_default(
            &conn,
            default_id,
            "Biên bản họp",
            "{transcript}",
            "vi",
            "meeting-minutes",
            0,
        )
        .unwrap();
        let user_id = MemoTemplateId::new();
        insert(&conn, user_id, "của tôi", "{transcript}", 1).unwrap();

        assert_eq!(delete_user_template(&conn, default_id).unwrap(), 0);
        assert!(get(&conn, default_id).unwrap().is_some());

        assert_eq!(delete_user_template(&conn, user_id).unwrap(), 1);
        assert!(get(&conn, user_id).unwrap().is_none());
    }

    #[test]
    fn update_name_prompt_returns_zero_rows_for_a_missing_id() {
        let conn = open_migrated();
        let affected =
            update_name_prompt(&conn, MemoTemplateId::new(), "x", "{transcript}", 0).unwrap();
        assert_eq!(affected, 0);
    }
}
