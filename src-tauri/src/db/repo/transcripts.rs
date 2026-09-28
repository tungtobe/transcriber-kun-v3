//! SQL cho bảng `transcripts` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL cho bảng này (spec Boundaries). `status` không bao
//! giờ nhận từ caller — [`insert_with_segments`]/[`replace_primary`] luôn tự
//! suy ra từ chính các segment được chèn (spec Always: "`transcripts.status`
//! do repo suy ra").

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::id::{SessionId, TranscriptId};

use super::segments::{self, GapReason, SegmentDraft, SegmentKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Retranscribe,
}

impl Variant {
    pub fn as_str(self) -> &'static str {
        match self {
            Variant::Primary => "primary",
            Variant::Retranscribe => "retranscribe",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "retranscribe" => Variant::Retranscribe,
            _ => Variant::Primary,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Complete,
    Partial,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Complete => "complete",
            Status::Partial => "partial",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "partial" => Status::Partial,
            _ => Status::Complete,
        }
    }
}

/// Một dòng `transcripts` đọc lại đầy đủ — dùng bởi Chạy lại (2.5) để kiểm
/// `transcript_id` thuộc đúng `session_id` trước khi giải scope, và để biết
/// `variant`/`status` hiện tại (spec Always: "Rust tự tra ... kiểm transcript
/// thuộc `session_id`").
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptRow {
    pub id: TranscriptId,
    pub session_id: SessionId,
    pub variant: Variant,
    pub status: Status,
    pub model: String,
    pub language: Option<String>,
}

/// Suy `status` từ danh sách segment sắp chèn: có ít nhất một gap
/// `chunk_failed` → `partial`; gap chỉ `disconnected` hoặc không có gap nào →
/// `complete` (spec I/O Matrix "Gap chunk_failed").
fn derive_status(segment_drafts: &[SegmentDraft]) -> Status {
    let has_chunk_failed_gap = segment_drafts.iter().any(|segment| {
        matches!(segment.kind, SegmentKind::Gap)
            && matches!(segment.gap_reason, Some(GapReason::ChunkFailed))
    });
    if has_chunk_failed_gap {
        Status::Partial
    } else {
        Status::Complete
    }
}

/// Chèn một transcript mới cùng toàn bộ segment của nó. Không tự mở
/// transaction — caller (`library::store::commit_file_session`/
/// `replace_primary_transcript`) bọc lời gọi này trong transaction của chính
/// nó, cùng với việc ghi `sessions` (spec Tasks: "trong transaction của
/// caller").
#[allow(clippy::too_many_arguments)]
pub fn insert_with_segments(
    conn: &Connection,
    id: TranscriptId,
    session_id: SessionId,
    variant: Variant,
    model: &str,
    language: Option<&str>,
    segment_drafts: &[SegmentDraft],
    created_at: i64,
) -> rusqlite::Result<Status> {
    let status = derive_status(segment_drafts);
    conn.execute(
        "INSERT INTO transcripts (id, session_id, variant, status, model, language, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id.to_string(),
            session_id.to_string(),
            variant.as_str(),
            status.as_str(),
            model,
            language,
            created_at,
        ],
    )?;
    for (idx, draft) in segment_drafts.iter().enumerate() {
        segments::insert(conn, id, idx as i64, draft)?;
    }
    Ok(status)
}

/// Appends the next ordered batch to an existing transcript. The unique
/// primary transcript belongs to its session and remains `partial` only if a
/// `chunk_failed` gap is present; disconnected live-audio gaps do not change
/// that status.
pub fn append_ordered_batch(
    conn: &Connection,
    id: TranscriptId,
    segment_drafts: &[SegmentDraft],
) -> rusqlite::Result<()> {
    if segment_drafts.is_empty() {
        return Ok(());
    }
    let first_idx = conn.query_row(
        "SELECT COALESCE(MAX(idx) + 1, 0) FROM segments WHERE transcript_id = ?1",
        params![id.to_string()],
        |row| row.get::<_, i64>(0),
    )?;
    segments::insert_ordered_batch(conn, id, first_idx, segment_drafts)?;
    if derive_status(segment_drafts) == Status::Partial {
        conn.execute(
            "UPDATE transcripts SET status = 'partial' WHERE id = ?1",
            params![id.to_string()],
        )?;
    }
    Ok(())
}

/// Chạy lại (retranscribe): xoá hẳn transcript `primary` hiện có của
/// `session_id` (kéo theo xoá segments của nó qua `ON DELETE CASCADE`), rồi
/// chèn bản `primary` mới — trong cùng transaction của caller (spec Design
/// Notes: "Chạy lại xoá hẳn primary cũ trong cùng transaction, không giữ
/// lịch sử"). Không có `primary` cũ (phiên mới) vẫn hoạt động bình thường,
/// `DELETE` không khớp dòng nào không phải lỗi.
pub fn replace_primary(
    conn: &Connection,
    session_id: SessionId,
    id: TranscriptId,
    model: &str,
    language: Option<&str>,
    segment_drafts: &[SegmentDraft],
    created_at: i64,
) -> rusqlite::Result<Status> {
    conn.execute(
        "DELETE FROM transcripts WHERE session_id = ?1 AND variant = 'primary'",
        params![session_id.to_string()],
    )?;
    insert_with_segments(
        conn,
        id,
        session_id,
        Variant::Primary,
        model,
        language,
        segment_drafts,
        created_at,
    )
}

fn parse_transcript_id(raw: &str) -> rusqlite::Result<TranscriptId> {
    TranscriptId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `transcripts.id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

fn parse_session_id(raw: &str) -> rusqlite::Result<SessionId> {
    SessionId::try_from(raw).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("giá trị `transcripts.session_id` không phải UUIDv7 hợp lệ: {raw}").into(),
        )
    })
}

fn row_to_transcript(row: &rusqlite::Row<'_>) -> rusqlite::Result<TranscriptRow> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let variant: String = row.get(2)?;
    let status: String = row.get(3)?;
    Ok(TranscriptRow {
        id: parse_transcript_id(&id)?,
        session_id: parse_session_id(&session_id)?,
        variant: Variant::from_str(&variant),
        status: Status::from_str(&status),
        model: row.get(4)?,
        language: row.get(5)?,
    })
}

/// Đọc một transcript theo id, `None` nếu không tồn tại (spec Tasks: "đọc
/// transcript theo id (session_id, variant, status, model, language)").
pub fn get(conn: &Connection, id: TranscriptId) -> rusqlite::Result<Option<TranscriptRow>> {
    conn.query_row(
        "SELECT id, session_id, variant, status, model, language FROM transcripts WHERE id = ?1",
        params![id.to_string()],
        row_to_transcript,
    )
    .optional()
}

/// Id của transcript `primary` hiện tại của một Phiên, `None` nếu chưa có
/// (spec Tasks: "transcript primary của Phiên").
pub fn primary_for_session(
    conn: &Connection,
    session_id: SessionId,
) -> rusqlite::Result<Option<TranscriptId>> {
    conn.query_row(
        "SELECT id FROM transcripts WHERE session_id = ?1 AND variant = 'primary'",
        params![session_id.to_string()],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| parse_transcript_id(&id))
    .transpose()
}

pub fn retranscribe_for_session(
    conn: &Connection,
    session_id: SessionId,
) -> rusqlite::Result<Option<TranscriptId>> {
    conn.query_row(
        "SELECT id FROM transcripts WHERE session_id = ?1 AND variant = 'retranscribe'",
        params![session_id.to_string()],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|id| parse_transcript_id(&id))
    .transpose()
}

/// Chạy lại (2.5): swap nguyên tử một transcript chỉ khi nó vẫn tồn tại với
/// đúng `expected_id` thuộc đúng `session_id` lúc bắt đầu -- trả `None`
/// (không ghi gì) nếu transcript đích đã bị xoá/đổi Phiên giữa chừng (spec
/// Always: "chỉ khi transcript đích vẫn tồn tại với đúng id lúc bắt đầu, xoá
/// nó và chèn bản mới cùng `variant`"; spec Design Notes: "swap so khớp
/// `expected_transcript_id` trong transaction thay vì khoá"). Bản mới giữ
/// đúng `variant` của bản cũ (không ép về `primary`) -- caller (`ipc::`) đã
/// chặn Chạy lại vào `primary` của Phiên `live` trước khi tới đây, nên hàm
/// này không tự phán đoán gì thêm về `variant`/`kind` Phiên. Không tự mở
/// transaction — caller (`library::store::swap_transcript`) bọc trong
/// transaction của chính nó, giống `insert_with_segments`/`replace_primary`.
#[allow(clippy::too_many_arguments)]
pub fn swap(
    conn: &Connection,
    session_id: SessionId,
    expected_id: TranscriptId,
    new_id: TranscriptId,
    model: &str,
    language: Option<&str>,
    segment_drafts: &[SegmentDraft],
    created_at: i64,
) -> rusqlite::Result<Option<Status>> {
    let Some(current) = get(conn, expected_id)? else {
        return Ok(None);
    };
    if current.session_id != session_id {
        return Ok(None);
    }
    conn.execute(
        "DELETE FROM transcripts WHERE id = ?1",
        params![expected_id.to_string()],
    )?;
    let status = insert_with_segments(
        conn,
        new_id,
        session_id,
        current.variant,
        model,
        language,
        segment_drafts,
        created_at,
    )?;
    Ok(Some(status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn open_migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    fn seed_session(conn: &Connection) -> SessionId {
        let session_id = SessionId::new();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES (?1, 'file', 't', 'complete', 1.0, 0, 0)",
            params![session_id.to_string()],
        )
        .unwrap();
        session_id
    }

    fn text_segment(start: f64, end: f64) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: "hello".to_string(),
            speaker: None,
        }
    }

    fn gap_segment(start: f64, end: f64, reason: GapReason) -> SegmentDraft {
        SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Gap,
            gap_reason: Some(reason),
            text: String::new(),
            speaker: None,
        }
    }

    #[test]
    fn insert_with_segments_derives_complete_when_no_gap() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let status = insert_with_segments(
            &conn,
            TranscriptId::new(),
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();
        assert_eq!(status, Status::Complete);
    }

    #[test]
    fn insert_with_segments_derives_complete_when_only_disconnected_gap() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let status = insert_with_segments(
            &conn,
            TranscriptId::new(),
            session_id,
            Variant::Primary,
            "m",
            None,
            &[
                text_segment(0.0, 1.0),
                gap_segment(1.0, 2.0, GapReason::Disconnected),
            ],
            0,
        )
        .unwrap();
        assert_eq!(status, Status::Complete);
    }

    #[test]
    fn insert_with_segments_derives_partial_when_chunk_failed_gap_present() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let status = insert_with_segments(
            &conn,
            TranscriptId::new(),
            session_id,
            Variant::Primary,
            "m",
            None,
            &[
                text_segment(0.0, 1.0),
                gap_segment(1.0, 2.0, GapReason::ChunkFailed),
            ],
            0,
        )
        .unwrap();
        assert_eq!(status, Status::Partial);
    }

    #[test]
    fn only_one_primary_transcript_survives_a_second_direct_insert() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        insert_with_segments(
            &conn,
            TranscriptId::new(),
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();
        let err = insert_with_segments(
            &conn,
            TranscriptId::new(),
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("unique"));
    }

    #[test]
    fn replace_primary_removes_old_primary_and_its_segments() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let old_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            old_id,
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0), text_segment(1.0, 2.0)],
            0,
        )
        .unwrap();

        let new_id = TranscriptId::new();
        let status = replace_primary(
            &conn,
            session_id,
            new_id,
            "m2",
            None,
            &[text_segment(0.0, 5.0)],
            10,
        )
        .unwrap();
        assert_eq!(status, Status::Complete);

        let old_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM transcripts WHERE id = ?1",
                params![old_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_count, 0, "primary cũ phải bị xoá hẳn");

        let old_segments: i64 = conn
            .query_row(
                "SELECT count(*) FROM segments WHERE transcript_id = ?1",
                params![old_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_segments, 0, "segments của primary cũ phải bị xoá theo");

        let new_segments = segments::list_for_transcript(&conn, new_id).unwrap();
        assert_eq!(new_segments.len(), 1);
    }

    #[test]
    fn replace_primary_on_session_without_an_existing_primary_still_inserts() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let status = replace_primary(
            &conn,
            session_id,
            TranscriptId::new(),
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();
        assert_eq!(status, Status::Complete);
    }

    #[test]
    fn get_reads_back_every_column_and_none_when_missing() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let id = TranscriptId::new();
        insert_with_segments(
            &conn,
            id,
            session_id,
            Variant::Retranscribe,
            "m",
            Some("vi"),
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();

        let row = get(&conn, id).unwrap().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.session_id, session_id);
        assert_eq!(row.variant, Variant::Retranscribe);
        assert_eq!(row.status, Status::Complete);
        assert_eq!(row.model, "m");
        assert_eq!(row.language.as_deref(), Some("vi"));

        assert!(get(&conn, TranscriptId::new()).unwrap().is_none());
    }

    #[test]
    fn primary_for_session_finds_only_the_primary_variant() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        assert!(primary_for_session(&conn, session_id).unwrap().is_none());

        let retranscribe_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            retranscribe_id,
            session_id,
            Variant::Retranscribe,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();
        assert!(primary_for_session(&conn, session_id).unwrap().is_none());

        let primary_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            primary_id,
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();
        assert_eq!(
            primary_for_session(&conn, session_id).unwrap(),
            Some(primary_id)
        );
    }

    #[test]
    fn swap_replaces_primary_when_expected_id_matches() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let old_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            old_id,
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0), text_segment(1.0, 2.0)],
            0,
        )
        .unwrap();

        let new_id = TranscriptId::new();
        let status = swap(
            &conn,
            session_id,
            old_id,
            new_id,
            "m2",
            None,
            &[text_segment(0.0, 5.0)],
            10,
        )
        .unwrap();
        assert_eq!(status, Some(Status::Complete));
        assert_eq!(
            primary_for_session(&conn, session_id).unwrap(),
            Some(new_id)
        );

        let old_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM transcripts WHERE id = ?1",
                params![old_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_count, 0);
    }

    #[test]
    fn swap_of_a_retranscribe_transcript_keeps_its_variant_and_leaves_primary_untouched() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let primary_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            primary_id,
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();
        let old_retranscribe_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            old_retranscribe_id,
            session_id,
            Variant::Retranscribe,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();

        let new_id = TranscriptId::new();
        let status = swap(
            &conn,
            session_id,
            old_retranscribe_id,
            new_id,
            "m2",
            None,
            &[text_segment(0.0, 5.0)],
            10,
        )
        .unwrap();
        assert_eq!(status, Some(Status::Complete));

        // The primary transcript is a completely different row and must be
        // untouched by swapping a `retranscribe` transcript.
        assert_eq!(
            primary_for_session(&conn, session_id).unwrap(),
            Some(primary_id)
        );

        let new_row = get(&conn, new_id).unwrap().unwrap();
        assert_eq!(
            new_row.variant,
            Variant::Retranscribe,
            "swap phải giữ nguyên variant của bản cũ, không ép về primary"
        );

        let old_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM transcripts WHERE id = ?1",
                params![old_retranscribe_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_count, 0, "bản retranscribe cũ phải bị xoá hẳn");
    }

    #[test]
    fn swap_returns_none_and_writes_nothing_when_expected_id_is_stale() {
        let conn = open_migrated();
        let session_id = seed_session(&conn);
        let real_primary_id = TranscriptId::new();
        insert_with_segments(
            &conn,
            real_primary_id,
            session_id,
            Variant::Primary,
            "m",
            None,
            &[text_segment(0.0, 1.0)],
            0,
        )
        .unwrap();

        let stale_id = TranscriptId::new();
        let result = swap(
            &conn,
            session_id,
            stale_id,
            TranscriptId::new(),
            "m2",
            None,
            &[text_segment(0.0, 1.0)],
            10,
        )
        .unwrap();
        assert_eq!(result, None);
        assert_eq!(
            primary_for_session(&conn, session_id).unwrap(),
            Some(real_primary_id),
            "primary hiện tại phải nguyên vẹn khi expected_id không khớp"
        );
    }
}
