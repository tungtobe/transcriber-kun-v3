//! SQL cho bảng `transcripts` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL cho bảng này (spec Boundaries). `status` không bao
//! giờ nhận từ caller — [`insert_with_segments`]/[`replace_primary`] luôn tự
//! suy ra từ chính các segment được chèn (spec Always: "`transcripts.status`
//! do repo suy ra").

use rusqlite::{params, Connection};

use crate::core::id::{SessionId, TranscriptId};

use super::segments::{self, GapReason, SegmentDraft, SegmentKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Retranscribe,
}

impl Variant {
    fn as_str(self) -> &'static str {
        match self {
            Variant::Primary => "primary",
            Variant::Retranscribe => "retranscribe",
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
}
