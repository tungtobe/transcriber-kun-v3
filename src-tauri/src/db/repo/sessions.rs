//! SQL cho bảng `sessions` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL cho bảng này (spec Boundaries).

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::{SessionId, TagId};

/// Tham số chèn một Phiên mới. `kind`/`status` là chuỗi thô khớp CHECK ở
/// migration (`"file"`/`"live"`, `"recording"`/`"finalizing"`/`"complete"`) —
/// Phiên file và live đều đi qua cùng repo helper; các ràng buộc nghiệp vụ
/// (bao gồm trạng thái hợp lệ theo từng loại) thuộc về `library::store`.
#[derive(Debug, Clone, Copy)]
pub struct NewSession<'a> {
    pub id: SessionId,
    pub kind: &'a str,
    pub title: &'a str,
    pub source_hash: Option<&'a str>,
    pub source_name: Option<&'a str>,
    pub status: &'a str,
    pub recovered: bool,
    pub duration_sec: f64,
    pub proxy_ext: Option<&'a str>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub id: SessionId,
    pub kind: String,
    pub title: String,
    pub source_hash: Option<String>,
    pub source_name: Option<String>,
    pub status: String,
    pub recovered: bool,
    pub duration_sec: f64,
    pub proxy_ext: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

const SELECT_COLUMNS: &str = "id, kind, title, source_hash, source_name, status, recovered, \
     duration_sec, proxy_ext, created_at, updated_at";

fn parse_session_id(raw: &str) -> rusqlite::Result<SessionId> {
    SessionId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `sessions.id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let id: String = row.get(0)?;
    Ok(SessionRow {
        id: parse_session_id(&id)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        source_hash: row.get(3)?,
        source_name: row.get(4)?,
        status: row.get(5)?,
        recovered: row.get::<_, i64>(6)? != 0,
        duration_sec: row.get(7)?,
        proxy_ext: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

/// Chèn một Phiên mới. Vi phạm CHECK/UNIQUE (`source_hash` trùng — spec I/O
/// Matrix "Trùng hash") trả thẳng `rusqlite::Error`, không nuốt lỗi — caller
/// (`library::store`) chịu trách nhiệm rollback transaction và gỡ Proxy vừa
/// publish.
pub fn insert(conn: &Connection, new: NewSession<'_>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO sessions (id, kind, title, source_hash, source_name, status, recovered, \
             duration_sec, proxy_ext, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            new.id.to_string(),
            new.kind,
            new.title,
            new.source_hash,
            new.source_name,
            new.status,
            new.recovered as i64,
            new.duration_sec,
            new.proxy_ext,
            new.created_at,
            new.updated_at,
        ],
    )?;
    Ok(())
}

/// Đọc một Phiên theo id, `None` nếu không tồn tại.
pub fn get(conn: &Connection, id: SessionId) -> rusqlite::Result<Option<SessionRow>> {
    conn.query_row(
        &format!("SELECT {SELECT_COLUMNS} FROM sessions WHERE id = ?1"),
        params![id.to_string()],
        row_to_session,
    )
    .optional()
}

/// Đọc toàn bộ Phiên, mới nhất trước.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<SessionRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLUMNS} FROM sessions ORDER BY created_at DESC"
    ))?;
    let rows = stmt.query_map([], row_to_session)?;
    rows.collect()
}

/// Cập nhật `proxy_ext` của một Phiên (`None` = `proxy_missing`, spec Design
/// Notes). Dùng bởi `library::store::commit_file_session` (Proxy lỗi lúc
/// commit) và `library::store::reconcile` (Proxy đã publish trước đó nhưng
/// file trên đĩa không còn — spec I/O Matrix "Crash giữa bước").
pub fn set_proxy_ext(
    conn: &Connection,
    id: SessionId,
    proxy_ext: Option<&str>,
    updated_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET proxy_ext = ?1, updated_at = ?2 WHERE id = ?3",
        params![proxy_ext, updated_at, id.to_string()],
    )?;
    Ok(())
}

/// Đổi tên một Phiên (story 3.1). `title` đã được caller
/// (`library::store::rename_session`) chuẩn hoá (trim, không rỗng, ≤ 200 ký
/// tự Unicode scalar) -- hàm này chỉ ghi thẳng, không validate lại. Trả số
/// dòng bị ảnh hưởng: `0` khi `id` không còn tồn tại (caller ánh xạ về
/// `Ok(None)`).
pub fn set_title(
    conn: &Connection,
    id: SessionId,
    title: &str,
    updated_at: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, updated_at, id.to_string()],
    )
}

/// Updates only a live session's sample-derived duration and lifecycle state.
/// Returns false when the row is missing or is not a Live session.
pub fn update_live_progress(
    conn: &Connection,
    id: SessionId,
    duration_sec: f64,
    status: &str,
    updated_at: i64,
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE sessions SET duration_sec = MAX(duration_sec, ?1), status = ?2, updated_at = ?3 \
         WHERE id = ?4 AND kind = 'live'",
        params![duration_sec.max(0.0), status, updated_at, id.to_string()],
    )?;
    Ok(changed == 1)
}

/// Atomically marks a Live session complete and records its final duration
/// and optional published Proxy. The row must already be `finalizing`, so a
/// failed finalization leaves the durable recovery state untouched.
pub fn finalize_live(
    conn: &Connection,
    id: SessionId,
    duration_sec: f64,
    proxy_ext: Option<&str>,
    updated_at: i64,
) -> rusqlite::Result<bool> {
    let changed = conn.execute(
        "UPDATE sessions SET duration_sec = MAX(duration_sec, ?1), proxy_ext = ?2, \
             status = 'complete', updated_at = ?3 \
         WHERE id = ?4 AND kind = 'live' AND status = 'finalizing'",
        params![duration_sec.max(0.0), proxy_ext, updated_at, id.to_string()],
    )?;
    Ok(changed == 1)
}

/// Xoá một Phiên (story 3.1, spec Approach). FK `ON DELETE CASCADE` (migration)
/// xoá luôn mọi `transcripts`/`segments` của nó -- `Db::open` đã bật
/// `foreign_keys` nên việc này chạy trong đúng lệnh `DELETE` này, không cần
/// xoá tay từng bảng. Idempotent: `id` không tồn tại xoá `0` dòng, không phải
/// lỗi (spec I/O Matrix "Xoá session không tồn tại").
pub fn delete(conn: &Connection, id: SessionId) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM sessions WHERE id = ?1",
        params![id.to_string()],
    )
}

/// Xoá một Phiên live khi khởi tạo Recording thất bại trước khi trả handle.
/// Điều kiện `kind = 'live'` giữ cleanup khỏi chạm nhầm Phiên file.
pub fn delete_live(conn: &Connection, id: SessionId) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM sessions WHERE id = ?1 AND kind = 'live'",
        params![id.to_string()],
    )
}

/// Đếm tổng số dòng `sessions` (story 3.4, spec Always: "số Phiên = số dòng
/// `sessions`") -- dùng bởi `library::store::storage_stats`.
pub fn count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
}

/// Xoá mọi Phiên (story 3.4, spec Approach "Xoá toàn bộ dữ liệu"). FK `ON
/// DELETE CASCADE` xoá luôn `transcripts`/`segments`/`session_tags` của mọi
/// Phiên trong cùng lệnh `DELETE` này -- không cần xoá tay từng bảng. Gọi
/// trong cùng transaction với [`super::tags::delete_all`] ở
/// `library::store::wipe_all` (spec Always: "DB trong một transaction").
/// Idempotent: kho rỗng xoá `0` dòng, không phải lỗi (spec I/O Matrix "Kho
/// rỗng").
pub fn delete_all(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM sessions", [])
}

/// Tra một Phiên theo `source_hash` chính xác (spec I/O Matrix "Trùng hash":
/// `transcribe_start` gọi trước khi tạo Job — trùng thì trả `Existing
/// { session_id }`, không tạo Job, không gọi Gemini). `source_hash` là
/// `UNIQUE` khi khác `NULL` nên tối đa một dòng khớp.
pub fn find_by_source_hash(
    conn: &Connection,
    source_hash: &str,
) -> rusqlite::Result<Option<SessionId>> {
    conn.query_row(
        "SELECT id FROM sessions WHERE source_hash = ?1",
        params![source_hash],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| parse_session_id(&id))
    .transpose()
}

/// Một dòng cho danh sách Home (story 2.9): đủ để vẽ dòng phiên (tên, kind,
/// ngày, thời lượng, badge `partial`/`recover`) mà không cần chi tiết
/// segment. `missing_gap_count` là số Segment gap `chunk_failed` của
/// transcript `primary` hiện tại của Phiên (`disconnected` không tính, và
/// Phiên chưa có `primary` -> 0) — spec I/O Matrix "Partial". `tag_ids` (story
/// 3.2) là mọi tag đang gắn với Phiên này, không thứ tự cụ thể — frontend tra
/// tên qua `tags_list` đã tải riêng.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionListRow {
    pub id: SessionId,
    pub kind: String,
    pub title: String,
    pub created_at: i64,
    pub duration_sec: f64,
    pub recovered: bool,
    pub missing_gap_count: i64,
    pub tag_ids: Vec<TagId>,
}

/// Đọc toàn bộ Phiên cho Home, mới nhất trước, tie-break `id` giảm dần (spec
/// Boundaries: "Sắp `created_at` giảm dần (tie-break theo `id` giảm dần)") —
/// UUIDv7 nên so sánh chuỗi cũng xấp xỉ thứ tự tạo. Truy vấn chính: `LEFT
/// JOIN transcripts` (chỉ variant `primary`) rồi `LEFT JOIN` một subquery đếm
/// `segments` gap `chunk_failed` theo `transcript_id`. Tag của mỗi Phiên (spec
/// Boundaries: "trả kèm tag của mỗi Phiên trong cùng truy vấn/không N+1")
/// tới từ một truy vấn thứ hai riêng — `repo::tags::list_all_links` đọc toàn
/// bộ `session_tags` một lần rồi gom theo `session_id` ở đây, không một truy
/// vấn tag/Phiên (N+1). Phiên chưa có transcript `primary` khớp `NULL` ở cả
/// hai JOIN, `COALESCE` về 0.
pub fn list_for_home(conn: &Connection) -> rusqlite::Result<Vec<SessionListRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.kind, s.title, s.created_at, s.duration_sec, s.recovered, \
             COALESCE(gap_counts.cnt, 0) AS missing_gap_count \
         FROM sessions s \
         LEFT JOIN transcripts t ON t.session_id = s.id AND t.variant = 'primary' \
         LEFT JOIN ( \
             SELECT transcript_id, COUNT(*) AS cnt \
             FROM segments \
             WHERE kind = 'gap' AND gap_reason = 'chunk_failed' \
             GROUP BY transcript_id \
         ) gap_counts ON gap_counts.transcript_id = t.id \
         ORDER BY s.created_at DESC, s.id DESC",
    )?;
    let mut rows: Vec<SessionListRow> = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(SessionListRow {
                id: parse_session_id(&id)?,
                kind: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                duration_sec: row.get(4)?,
                recovered: row.get::<_, i64>(5)? != 0,
                missing_gap_count: row.get(6)?,
                tag_ids: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut tags_by_session: HashMap<String, Vec<TagId>> = HashMap::new();
    for (session_id, tag_id) in super::tags::list_all_links(conn)? {
        tags_by_session.entry(session_id).or_default().push(tag_id);
    }
    for row in &mut rows {
        if let Some(tag_ids) = tags_by_session.remove(&row.id.to_string()) {
            row.tag_ids = tag_ids;
        }
    }

    Ok(rows)
}

/// Đọc `(id, kind, proxy_ext)` của mọi Phiên — dùng bởi
/// `library::store::reconcile` để biết thư mục `media/<id>` nào có dòng DB
/// tham chiếu, loại Phiên nào được phép giữ Recording, và Proxy nào đang
/// được tham chiếu, không cần toàn bộ cột khác của [`SessionRow`].
pub fn list_media_refs(
    conn: &Connection,
) -> rusqlite::Result<Vec<(SessionId, String, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT id, kind, proxy_ext FROM sessions")?;
    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let kind: String = row.get(1)?;
        let proxy_ext: Option<String> = row.get(2)?;
        Ok((id, kind, proxy_ext))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, kind, proxy_ext) = row?;
        out.push((parse_session_id(&id)?, kind, proxy_ext));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::db::repo::segments::GapReason;

    fn open_migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    fn sample(id: SessionId) -> NewSession<'static> {
        NewSession {
            id,
            kind: "file",
            title: "cuộc họp",
            source_hash: Some("hash-1"),
            source_name: Some("meeting.mp4"),
            status: "complete",
            recovered: false,
            duration_sec: 12.5,
            proxy_ext: Some("flac"),
            created_at: 1_000,
            updated_at: 1_000,
        }
    }

    #[test]
    fn insert_then_get_round_trips_all_columns() {
        let conn = open_migrated();
        let id = SessionId::new();
        insert(&conn, sample(id)).unwrap();

        let row = get(&conn, id).unwrap().expect("phải đọc lại được");
        assert_eq!(row.id, id);
        assert_eq!(row.kind, "file");
        assert_eq!(row.title, "cuộc họp");
        assert_eq!(row.source_hash.as_deref(), Some("hash-1"));
        assert_eq!(row.source_name.as_deref(), Some("meeting.mp4"));
        assert_eq!(row.status, "complete");
        assert!(!row.recovered);
        assert!((row.duration_sec - 12.5).abs() < f64::EPSILON);
        assert_eq!(row.proxy_ext.as_deref(), Some("flac"));
        assert_eq!(row.created_at, 1_000);
        assert_eq!(row.updated_at, 1_000);
    }

    #[test]
    fn get_on_missing_id_returns_none() {
        let conn = open_migrated();
        assert!(get(&conn, SessionId::new()).unwrap().is_none());
    }

    #[test]
    fn list_returns_every_session() {
        let conn = open_migrated();
        let a = SessionId::new();
        let b = SessionId::new();
        insert(&conn, sample(a)).unwrap();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                ..sample(b)
            },
        )
        .unwrap();
        let rows = list(&conn).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn duplicate_source_hash_is_rejected() {
        let conn = open_migrated();
        insert(&conn, sample(SessionId::new())).unwrap();
        let err = insert(&conn, sample(SessionId::new())).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn finalize_live_updates_completion_fields_only_from_finalizing_state() {
        let conn = open_migrated();
        let id = SessionId::new();
        insert(
            &conn,
            NewSession {
                kind: "live",
                status: "finalizing",
                source_hash: None,
                source_name: None,
                proxy_ext: None,
                ..sample(id)
            },
        )
        .unwrap();

        assert!(finalize_live(&conn, id, 20.0, Some("flac"), 2_000).unwrap());
        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(row.proxy_ext.as_deref(), Some("flac"));
        assert_eq!(row.duration_sec, 20.0);
        assert_eq!(row.updated_at, 2_000);
        assert!(!finalize_live(&conn, id, 25.0, None, 3_000).unwrap());
        assert_eq!(get(&conn, id).unwrap().unwrap().duration_sec, 20.0);
    }

    #[test]
    fn null_source_hash_does_not_collide() {
        let conn = open_migrated();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                ..sample(SessionId::new())
            },
        )
        .unwrap();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                ..sample(SessionId::new())
            },
        )
        .unwrap();
        assert_eq!(list(&conn).unwrap().len(), 2);
    }

    #[test]
    fn set_proxy_ext_updates_proxy_ext_and_updated_at() {
        let conn = open_migrated();
        let id = SessionId::new();
        insert(&conn, sample(id)).unwrap();

        set_proxy_ext(&conn, id, None, 2_000).unwrap();

        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.proxy_ext, None);
        assert_eq!(row.updated_at, 2_000);
    }

    #[test]
    fn set_title_updates_title_and_updated_at() {
        let conn = open_migrated();
        let id = SessionId::new();
        insert(&conn, sample(id)).unwrap();

        let affected = set_title(&conn, id, "Họp sprint 12", 2_000).unwrap();
        assert_eq!(affected, 1);

        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.title, "Họp sprint 12");
        assert_eq!(row.updated_at, 2_000);
    }

    #[test]
    fn set_title_on_missing_id_affects_zero_rows() {
        let conn = open_migrated();
        assert_eq!(set_title(&conn, SessionId::new(), "x", 1).unwrap(), 0);
    }

    #[test]
    fn delete_on_missing_id_affects_zero_rows() {
        let conn = open_migrated_fk();
        assert_eq!(delete(&conn, SessionId::new()).unwrap(), 0);
    }

    #[test]
    fn delete_removes_the_session_row_and_cascades_to_transcripts_and_segments() {
        let conn = open_migrated_fk();
        let id = SessionId::new();
        insert_session_at(&conn, id, 1_000);
        insert_primary_with_segments(&conn, id, &[text_seg(0.0, 1.0)]);

        let affected = delete(&conn, id).unwrap();
        assert_eq!(affected, 1);

        assert!(get(&conn, id).unwrap().is_none());
        let transcript_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transcripts WHERE session_id = ?1",
                params![id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(transcript_count, 0, "cascade phải xoá transcript");
        let segment_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM segments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(segment_count, 0, "cascade phải xoá segment");
    }

    #[test]
    fn delete_does_not_touch_other_sessions() {
        let conn = open_migrated_fk();
        let kept = SessionId::new();
        let removed = SessionId::new();
        insert_session_at(&conn, kept, 1_000);
        insert_session_at(&conn, removed, 2_000);

        delete(&conn, removed).unwrap();

        assert!(get(&conn, kept).unwrap().is_some());
        assert!(get(&conn, removed).unwrap().is_none());
    }

    #[test]
    fn find_by_source_hash_finds_the_exact_match_and_none_when_absent() {
        let conn = open_migrated();
        let a = SessionId::new();
        insert(
            &conn,
            NewSession {
                source_hash: Some("hash-a"),
                ..sample(a)
            },
        )
        .unwrap();

        assert_eq!(find_by_source_hash(&conn, "hash-a").unwrap(), Some(a));
        assert_eq!(find_by_source_hash(&conn, "hash-missing").unwrap(), None);
    }

    fn open_migrated_fk() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    fn insert_session_at(conn: &Connection, id: SessionId, created_at: i64) {
        insert(
            conn,
            NewSession {
                id,
                source_hash: None,
                created_at,
                updated_at: created_at,
                ..sample(id)
            },
        )
        .unwrap();
    }

    fn insert_primary_with_segments(
        conn: &Connection,
        session_id: SessionId,
        segment_drafts: &[crate::db::repo::segments::SegmentDraft],
    ) {
        crate::db::repo::transcripts::insert_with_segments(
            conn,
            crate::core::id::TranscriptId::new(),
            session_id,
            crate::db::repo::transcripts::Variant::Primary,
            "m",
            None,
            segment_drafts,
            0,
        )
        .unwrap();
    }

    fn text_seg(start: f64, end: f64) -> crate::db::repo::segments::SegmentDraft {
        crate::db::repo::segments::SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: crate::db::repo::segments::SegmentKind::Text,
            gap_reason: None,
            text: "hi".to_string(),
            speaker: None,
        }
    }

    fn gap_seg(
        start: f64,
        end: f64,
        reason: crate::db::repo::segments::GapReason,
    ) -> crate::db::repo::segments::SegmentDraft {
        crate::db::repo::segments::SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: crate::db::repo::segments::SegmentKind::Gap,
            gap_reason: Some(reason),
            text: String::new(),
            speaker: None,
        }
    }

    #[test]
    fn list_for_home_orders_newest_created_at_first_tie_broken_by_id_desc() {
        let conn = open_migrated_fk();
        let older = SessionId::new();
        let newer_a = SessionId::new();
        let newer_b = SessionId::new();
        insert_session_at(&conn, older, 1_000);
        // Cùng `created_at` -- tie-break theo `id` giảm dần.
        let (first_by_id, second_by_id) = if newer_a.to_string() > newer_b.to_string() {
            (newer_a, newer_b)
        } else {
            (newer_b, newer_a)
        };
        insert_session_at(&conn, first_by_id, 2_000);
        insert_session_at(&conn, second_by_id, 2_000);

        let rows = list_for_home(&conn).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![first_by_id, second_by_id, older]
        );
    }

    #[test]
    fn list_for_home_counts_only_chunk_failed_gaps_of_the_primary_transcript() {
        let conn = open_migrated_fk();
        let id = SessionId::new();
        insert_session_at(&conn, id, 1_000);
        insert_primary_with_segments(
            &conn,
            id,
            &[
                text_seg(0.0, 1.0),
                gap_seg(1.0, 2.0, GapReason::ChunkFailed),
                gap_seg(2.0, 3.0, GapReason::Disconnected),
                gap_seg(3.0, 4.0, GapReason::ChunkFailed),
            ],
        );

        let rows = list_for_home(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].missing_gap_count, 2, "disconnected không tính");
    }

    #[test]
    fn list_for_home_ignores_gaps_belonging_to_a_retranscribe_variant() {
        let conn = open_migrated_fk();
        let id = SessionId::new();
        insert_session_at(&conn, id, 1_000);
        crate::db::repo::transcripts::insert_with_segments(
            &conn,
            crate::core::id::TranscriptId::new(),
            id,
            crate::db::repo::transcripts::Variant::Primary,
            "m",
            None,
            &[text_seg(0.0, 1.0)],
            0,
        )
        .unwrap();
        crate::db::repo::transcripts::insert_with_segments(
            &conn,
            crate::core::id::TranscriptId::new(),
            id,
            crate::db::repo::transcripts::Variant::Retranscribe,
            "m",
            None,
            &[gap_seg(1.0, 2.0, GapReason::ChunkFailed)],
            0,
        )
        .unwrap();

        let rows = list_for_home(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].missing_gap_count, 0,
            "gap của transcript retranscribe không được tính vào Home"
        );
    }

    #[test]
    fn list_for_home_session_without_any_transcript_has_zero_missing_gaps() {
        let conn = open_migrated_fk();
        let id = SessionId::new();
        insert_session_at(&conn, id, 1_000);

        let rows = list_for_home(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].missing_gap_count, 0);
        assert_eq!(rows[0].id, id);
    }

    #[test]
    fn list_for_home_reads_500_sessions_quickly_and_in_order() {
        let conn = open_migrated_fk();
        let mut ids = Vec::with_capacity(500);
        for i in 0..500i64 {
            let id = SessionId::new();
            insert_session_at(&conn, id, 1_000 + i);
            ids.push(id);
        }

        let started = std::time::Instant::now();
        let rows = list_for_home(&conn).unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "500 Phiên phải đọc nhanh (một truy vấn, không N+1)"
        );
        assert_eq!(rows.len(), 500);
        // Mới nhất trước -> id cuối cùng chèn (created_at lớn nhất) đứng đầu.
        assert_eq!(rows[0].id, *ids.last().unwrap());
        assert_eq!(rows[499].id, ids[0]);
    }

    #[test]
    fn list_for_home_includes_tag_ids_of_each_session_without_n_plus_one() {
        let conn = open_migrated_fk();
        let tagged = SessionId::new();
        let untagged = SessionId::new();
        insert_session_at(&conn, tagged, 1_000);
        insert_session_at(&conn, untagged, 2_000);
        let tag = crate::db::repo::tags::upsert_by_name_key(
            &conn,
            crate::core::id::TagId::new(),
            "a",
            "a",
            0,
        )
        .unwrap();
        crate::db::repo::tags::attach(&conn, tagged, tag.id).unwrap();

        let rows = list_for_home(&conn).unwrap();
        let tagged_row = rows.iter().find(|r| r.id == tagged).unwrap();
        let untagged_row = rows.iter().find(|r| r.id == untagged).unwrap();
        assert_eq!(tagged_row.tag_ids, vec![tag.id]);
        assert!(untagged_row.tag_ids.is_empty());
    }

    #[test]
    fn list_media_refs_returns_id_and_proxy_ext_pairs() {
        let conn = open_migrated();
        let a = SessionId::new();
        let b = SessionId::new();
        insert(&conn, sample(a)).unwrap();
        insert(
            &conn,
            NewSession {
                source_hash: None,
                proxy_ext: None,
                ..sample(b)
            },
        )
        .unwrap();

        let mut refs = list_media_refs(&conn).unwrap();
        refs.sort_by_key(|(id, _, _)| id.to_string());
        let mut expected = vec![
            (a, "file".to_string(), Some("flac".to_string())),
            (b, "file".to_string(), None),
        ];
        expected.sort_by_key(|(id, _, _)| id.to_string());
        assert_eq!(refs, expected);
    }
}
