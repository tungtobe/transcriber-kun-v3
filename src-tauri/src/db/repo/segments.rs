//! SQL cho bảng `segments` — nơi duy nhất khác ngoài `db/migrations/mod.rs`
//! được phép chứa câu SQL cho bảng này (spec Boundaries). `idx` là vị trí của
//! đoạn trong transcript (0-based, thứ tự chèn) — caller (`transcripts::`)
//! gán, module này không tự đánh số.

use rusqlite::{params, Connection};

use crate::core::id::TranscriptId;

/// Loại một dòng `segments`: đoạn văn bản đã nhận diện, hoặc một khoảng
/// thiếu (gap) không transcribe được.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Text,
    Gap,
}

impl SegmentKind {
    fn as_str(self) -> &'static str {
        match self {
            SegmentKind::Text => "text",
            SegmentKind::Gap => "gap",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "gap" => SegmentKind::Gap,
            _ => SegmentKind::Text,
        }
    }
}

/// Lý do một gap tồn tại — CHECK ở migration chỉ chấp nhận đúng hai giá trị
/// này, khớp 1-1 với các biến thể ở đây.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapReason {
    ChunkFailed,
    Disconnected,
}

impl GapReason {
    fn as_str(self) -> &'static str {
        match self {
            GapReason::ChunkFailed => "chunk_failed",
            GapReason::Disconnected => "disconnected",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "chunk_failed" => Some(GapReason::ChunkFailed),
            "disconnected" => Some(GapReason::Disconnected),
            _ => None,
        }
    }
}

/// Một đoạn đã chuẩn hoá, sẵn sàng ghi — caller (job 2.4/2.5) đã ánh xạ
/// `transcribe::parser::Segment`/`MissingRange` sang dạng này trước khi gọi
/// vào `db`/`library` (spec Code Map: "story này chỉ nhận draft đã chuẩn
/// hoá").
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentDraft {
    pub start_sec: f64,
    pub end_sec: f64,
    pub kind: SegmentKind,
    pub gap_reason: Option<GapReason>,
    pub text: String,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRow {
    pub idx: i64,
    pub start_sec: f64,
    pub end_sec: f64,
    pub kind: SegmentKind,
    pub gap_reason: Option<GapReason>,
    pub text: String,
    pub speaker: Option<String>,
}

/// Chèn một đoạn ở vị trí `idx` của `transcript_id`. Không tự transaction —
/// caller (`transcripts::insert_with_segments`/`replace_primary`) gọi lặp
/// trong transaction của chính nó (spec Tasks).
pub fn insert(
    conn: &Connection,
    transcript_id: TranscriptId,
    idx: i64,
    draft: &SegmentDraft,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO segments (transcript_id, idx, start_sec, end_sec, kind, gap_reason, text, speaker) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            transcript_id.to_string(),
            idx,
            draft.start_sec,
            draft.end_sec,
            draft.kind.as_str(),
            draft.gap_reason.map(GapReason::as_str),
            draft.text,
            draft.speaker,
        ],
    )?;
    Ok(())
}

/// Đọc mọi đoạn của một transcript, theo đúng thứ tự `idx`.
pub fn list_for_transcript(
    conn: &Connection,
    transcript_id: TranscriptId,
) -> rusqlite::Result<Vec<SegmentRow>> {
    let mut stmt = conn.prepare(
        "SELECT idx, start_sec, end_sec, kind, gap_reason, text, speaker \
         FROM segments WHERE transcript_id = ?1 ORDER BY idx",
    )?;
    let rows = stmt.query_map(params![transcript_id.to_string()], |row| {
        let kind: String = row.get(3)?;
        let gap_reason: Option<String> = row.get(4)?;
        Ok(SegmentRow {
            idx: row.get(0)?,
            start_sec: row.get(1)?,
            end_sec: row.get(2)?,
            kind: SegmentKind::from_str(&kind),
            gap_reason: gap_reason.as_deref().and_then(GapReason::from_str),
            text: row.get(5)?,
            speaker: row.get(6)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::id::{SessionId, TranscriptId};
    use crate::db::migrations;

    fn open_migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    fn seed_transcript(conn: &Connection) -> TranscriptId {
        let session_id = SessionId::new();
        let transcript_id = TranscriptId::new();
        conn.execute(
            "INSERT INTO sessions (id, kind, title, status, duration_sec, created_at, updated_at) \
             VALUES (?1, 'file', 't', 'complete', 1.0, 0, 0)",
            params![session_id.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transcripts (id, session_id, variant, status, model, created_at) \
             VALUES (?1, ?2, 'primary', 'complete', 'm', 0)",
            params![transcript_id.to_string(), session_id.to_string()],
        )
        .unwrap();
        transcript_id
    }

    #[test]
    fn insert_then_list_round_trips_text_and_gap_segments() {
        let conn = open_migrated();
        let transcript_id = seed_transcript(&conn);

        insert(
            &conn,
            transcript_id,
            0,
            &SegmentDraft {
                start_sec: 0.0,
                end_sec: 1.5,
                kind: SegmentKind::Text,
                gap_reason: None,
                text: "xin chào".to_string(),
                speaker: Some("A".to_string()),
            },
        )
        .unwrap();
        insert(
            &conn,
            transcript_id,
            1,
            &SegmentDraft {
                start_sec: 1.5,
                end_sec: 3.0,
                kind: SegmentKind::Gap,
                gap_reason: Some(GapReason::ChunkFailed),
                text: String::new(),
                speaker: None,
            },
        )
        .unwrap();

        let rows = list_for_transcript(&conn, transcript_id).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, SegmentKind::Text);
        assert_eq!(rows[0].text, "xin chào");
        assert_eq!(rows[0].speaker.as_deref(), Some("A"));
        assert_eq!(rows[1].kind, SegmentKind::Gap);
        assert_eq!(rows[1].gap_reason, Some(GapReason::ChunkFailed));
    }

    #[test]
    fn list_for_transcript_on_transcript_without_segments_is_empty() {
        let conn = open_migrated();
        let transcript_id = seed_transcript(&conn);
        assert!(list_for_transcript(&conn, transcript_id)
            .unwrap()
            .is_empty());
    }
}
