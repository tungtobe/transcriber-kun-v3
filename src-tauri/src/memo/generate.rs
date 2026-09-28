//! Sinh memo (story 3.7): chụp đầu vào, dựng prompt, gọi
//! `GeminiGateway::post_job_observed_for(ModelKind::Memo, ...)`, commit chỉ
//! khi thành công. Chủ ghi duy nhất cho `memos` ngoài `db/repo/memos.rs` --
//! `ipc::` gọi qua đây (`run`/`view`), không gọi thẳng `repo::memos`.
//!
//! Đọc lại (`memo_get`) không bao giờ gọi Gemini (spec Boundaries Always:
//! "Mở lại memo chỉ đọc DB") -- [`view`] chỉ đọc `db::repo::memos` +
//! transcript/ghi chú hiện tại để suy hai cờ provenance.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::error::{AppError, Code};
use crate::core::id::{MemoTemplateId, SessionId, TranscriptId};
use crate::core::sensitive::Sensitive;
use crate::db::repo::{
    self,
    segments::{SegmentKind, SegmentRow},
    transcripts::Status as TranscriptStatus,
};
use crate::db::Db;
use crate::gemini::{CancellationToken, ConsentSnapshot, GeminiGateway, ModelKind};
use crate::library::export::{display_seconds, padded_timestamp};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Locale hỗ trợ cho chuỗi prompt (spec Boundaries Always: "Chuỗi này sống
/// trong Rust (`memo/`), locale `vi|en|ja`") -- cố ý tách khỏi
/// `memo::templates`' locale validation (đó là cho *danh sách template*,
/// đây là cho *nội dung chèn vào prompt*), dù cùng 3 giá trị.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Locale {
    Vi,
    En,
    Ja,
}

impl Locale {
    fn parse(raw: &str) -> Result<Self, AppError> {
        match raw {
            "vi" => Ok(Self::Vi),
            "en" => Ok(Self::En),
            "ja" => Ok(Self::Ja),
            _ => Err(AppError::new(
                Code::Request,
                format!("Locale không được hỗ trợ: {raw}"),
            )),
        }
    }
}

fn notes_header(locale: Locale) -> &'static str {
    match locale {
        Locale::Vi => "Ghi chú của người dùng:",
        Locale::En => "User notes:",
        Locale::Ja => "ユーザーのメモ:",
    }
}

fn notes_empty_text(locale: Locale) -> &'static str {
    match locale {
        Locale::Vi => "(không có ghi chú)",
        Locale::En => "(no notes)",
        Locale::Ja => "(メモなし)",
    }
}

fn gap_message(locale: Locale) -> &'static str {
    match locale {
        Locale::Vi => "(Thiếu nội dung ở khoảng này)",
        Locale::En => "(Missing content for this range)",
        Locale::Ja => "（この区間は内容が欠落しています）",
    }
}

fn partial_prefix_sentence(locale: Locale, ranges: &[String]) -> String {
    let joined = ranges.join(", ");
    match locale {
        Locale::Vi => format!("Bản ghi bị thiếu nội dung ở các khoảng: {joined}."),
        Locale::En => format!("The transcript is missing content during: {joined}."),
        Locale::Ja => format!("文字起こしには次の時間帯の内容が欠落しています: {joined}。"),
    }
}

/// Thay `{transcript}`: mỗi đoạn text một dòng `[mm:ss] text` (offset hiển
/// thị như export 2.10, spec Boundaries Always); mỗi gap một dòng thông báo
/// thiếu nội dung theo locale kèm khoảng thời gian. Khi transcript `partial`
/// **và** có ít nhất một gap, chèn trước một câu liệt kê mọi khoảng thiếu
/// (spec: "nếu transcript partial thì chèn trước một câu liệt kê các khoảng
/// thiếu").
fn format_transcript_block(
    segments: &[SegmentRow],
    status: TranscriptStatus,
    offset_sec: f64,
    locale: Locale,
) -> String {
    let mut lines = Vec::with_capacity(segments.len());
    let mut gap_ranges = Vec::new();
    for segment in segments {
        let start = display_seconds(segment.start_sec, offset_sec);
        match segment.kind {
            SegmentKind::Text => {
                lines.push(format!("[{}] {}", padded_timestamp(start), segment.text));
            }
            SegmentKind::Gap => {
                let end = display_seconds(segment.end_sec, offset_sec);
                let range = format!("{}–{}", padded_timestamp(start), padded_timestamp(end));
                lines.push(format!("[{range}] {}", gap_message(locale)));
                gap_ranges.push(range);
            }
        }
    }
    let body = lines.join("\n");
    if status == TranscriptStatus::Partial && !gap_ranges.is_empty() {
        format!("{}\n\n{body}", partial_prefix_sentence(locale, &gap_ranges))
    } else {
        body
    }
}

/// Thay `{transcript}` luôn luôn; `{notes}` chỉ khi `template_prompt` thật sự
/// chứa token đó (spec Boundaries Always: "`{notes}` (nếu có trong
/// template)") -- một template không dùng `{notes}` không bao giờ chạm tới
/// nội dung ghi chú, kể cả khi Phiên có ghi chú.
fn build_prompt(
    template_prompt: &str,
    transcript_block: &str,
    notes_body: Option<&str>,
    locale: Locale,
) -> String {
    let mut prompt = template_prompt.replace("{transcript}", transcript_block);
    if template_prompt.contains("{notes}") {
        let notes_block = match notes_body.map(str::trim) {
            Some(body) if !body.is_empty() => format!("{}\n{body}", notes_header(locale)),
            _ => notes_empty_text(locale).to_string(),
        };
        prompt = prompt.replace("{notes}", &notes_block);
    }
    prompt
}

/// Cùng quy tắc ký tự an toàn với `transcribe::adapter::model_id` (một bản
/// tách riêng, không tái dùng cái đó vì Memo không áp hai điều kiện loại trừ
/// "transcribe"/"live-translate" của file transcription -- `memo_model` là
/// một trường tự do độc lập, spec Code Map không nói gì về việc chặn tên).
fn model_id(model: &str) -> Result<&str, AppError> {
    let id = model.strip_prefix("models/").unwrap_or(model);
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::new(Code::Model, "Gemini model ID is invalid"));
    }
    Ok(id)
}

/// Body text-only `{"contents":[{"role":"user","parts":[{"text":prompt}]}]}`
/// (spec Boundaries Always) -- không `generationConfig`/schema nào, khác hẳn
/// `transcribe::adapter::build_general_request`.
fn build_request(model: &str, prompt: &str) -> Result<(String, String), AppError> {
    let model = model_id(model)?;
    let body = json!({
        "contents": [{"role": "user", "parts": [{"text": prompt}]}],
    });
    let serialized = serde_json::to_string(&body)
        .map_err(|error| AppError::new(Code::Format, error.to_string()))?;
    Ok((
        format!("/v1beta/models/{model}:generateContent"),
        serialized,
    ))
}

fn shape() -> AppError {
    AppError::new(Code::Shape, "Gemini memo response is incomplete or invalid")
}

/// Trích text Markdown từ một response `generateContent` -- không parse JSON
/// schema (memo là văn xuôi tự do, khác `transcribe::parser`).
fn parse_memo_response(body: &str) -> Result<String, AppError> {
    let response: Value = serde_json::from_str(body).map_err(|_| shape())?;
    if response.pointer("/promptFeedback/blockReason").is_some() {
        return Err(AppError::new(Code::Blocked, "Gemini blocked the memo"));
    }
    let candidate = response.pointer("/candidates/0").ok_or_else(shape)?;
    if candidate.get("finishReason").and_then(Value::as_str) == Some("SAFETY") {
        return Err(AppError::new(Code::Blocked, "Gemini blocked the memo"));
    }
    let text = candidate
        .pointer("/content/parts/0/text")
        .and_then(Value::as_str)
        .ok_or_else(shape)?;
    Ok(text.to_string())
}

struct CapturedInputs {
    transcript_id: TranscriptId,
    transcript_status: TranscriptStatus,
    segments: Vec<SegmentRow>,
    notes_body: Option<String>,
    notes_revision: Option<i64>,
    template_name: String,
    template_prompt: String,
    model: String,
    timestamp_offset_sec: f64,
}

/// Chụp mọi đầu vào lúc bắt đầu (spec Boundaries Always: "chụp đầu vào lúc
/// bắt đầu"); một thay đổi transcript/ghi chú/template *sau* thời điểm này
/// không ảnh hưởng lần sinh đang chạy. Trả lỗi `Request` cho ba tình huống
/// I/O Matrix chặn nút Sinh phía UI: chưa có transcript primary, transcript
/// chỉ toàn gap (không đoạn text nào), hoặc template không còn tồn tại.
fn capture_inputs(
    db: &Db,
    session_id: SessionId,
    template_id: MemoTemplateId,
    selected_transcript_id: Option<TranscriptId>,
) -> Result<CapturedInputs, AppError> {
    let settings = crate::settings::load(db);
    db.with_connection(|conn| {
        let transcript_id = selected_transcript_id.or(repo::transcripts::primary_for_session(conn, session_id)?)
            .ok_or_else(|| AppError::new(Code::Request, "Phiên chưa có transcript"))?;
        let transcript = repo::transcripts::get(conn, transcript_id)?
            .ok_or_else(|| AppError::new(Code::Request, "Phiên chưa có transcript"))?;
        if transcript.session_id != session_id {
            return Err(AppError::new(Code::Request, "Transcript không thuộc Phiên này"));
        }
        let segments = repo::segments::list_for_transcript(conn, transcript_id)?;
        if segments
            .iter()
            .all(|segment| segment.kind == SegmentKind::Gap)
        {
            return Err(AppError::new(
                Code::Request,
                "Transcript không có đoạn text nào để sinh memo",
            ));
        }
        let notes = repo::notes::get(conn, session_id)?;
        let template = repo::memo_templates::get(conn, template_id)?
            .ok_or_else(|| AppError::new(Code::Request, "Template memo không tồn tại"))?;
        Ok(CapturedInputs {
            transcript_id,
            transcript_status: transcript.status,
            segments,
            notes_body: notes.as_ref().map(|row| row.body.expose().clone()),
            notes_revision: notes.map(|row| row.revision),
            template_name: template.name,
            template_prompt: template.prompt,
            model: settings.memo_model.clone(),
            timestamp_offset_sec: settings.timestamp_offset_sec as f64,
        })
    })
}

/// Một memo qua IPC (`memo_generate`/`memo_get`) -- `body` là Markdown thô,
/// chưa sanitize (spec Boundaries Always: "Render Markdown bằng `marked` rồi
/// `DOMPurify.sanitize`" ở frontend, không phải ở đây).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MemoView {
    pub body: String,
    /// Mili-giây kể từ Unix epoch (UTC), `f64` -- cùng quy ước
    /// `SessionDetail::created_at`.
    pub created_at: f64,
    /// Chuỗi thô `"complete" | "partial"` của transcript đã dùng để sinh --
    /// nguồn cho dòng "Sinh từ bản ... đầy đủ|thiếu".
    pub transcript_status: String,
    pub model: String,
    pub template_name: String,
    /// `true` khi template đã dùng để sinh có chứa `{notes}` -- frontend bỏ
    /// "+ ghi chú" khỏi dòng nguồn khi `false` (spec Boundaries Always).
    pub uses_notes: bool,
    /// `true` khi transcript primary hiện tại của Phiên khác transcript đã
    /// dùng để sinh memo này (Chạy lại tạo `TranscriptId` mới -- so khớp id
    /// là đủ, spec Design Notes).
    pub from_previous_transcript: bool,
    /// `true` khi revision ghi chú hiện tại khác revision đã chụp lúc sinh.
    pub notes_changed: bool,
}

/// Kết quả [`run`] -- `AlreadyRunning` không thuộc về đây (quyết định trước
/// khi gọi `run`, ở registry của `ipc::`, spec Boundaries Always: "registry
/// ... chỉ `ipc/` điều phối"). `Cancelled` gộp cả huỷ tường minh
/// (`memo_cancel`) lẫn "Phiên bị xoá giữa lúc đang sinh" (spec: "commit sau
/// đó ... coi như `Cancelled`").
#[derive(Debug, Clone, PartialEq)]
pub enum GenerateOutcome {
    Generated(MemoView),
    Cancelled,
}

/// Sinh (hoặc sinh lại) một memo -- xem module doc cho luồng đầy đủ. Chỉ
/// commit khi gateway trả thành công **và** request chưa bị huỷ lúc đó
/// (kiểm `cancellation.is_cancelled()` lại sau await, spec I/O Matrix "Kết
/// quả tới sau khi đã huỷ ... bị bỏ"); mọi lỗi khác (quota/auth/network/
/// timeout/model/…) trả nguyên `AppError` của gateway, memo cũ giữ nguyên vì
/// không đường nào ở đây từng chạm `repo::memos::upsert`.
pub async fn run(
    db: &Db,
    gateway: &GeminiGateway,
    session_id: SessionId,
    template_id: MemoTemplateId,
    locale: &str,
    consent: ConsentSnapshot,
    cancellation: CancellationToken,
) -> Result<GenerateOutcome, AppError> {
    run_for_source(
        db,
        gateway,
        session_id,
        template_id,
        None,
        locale,
        consent,
        cancellation,
    )
    .await
}

/// Generate a memo from the transcript selected in Session detail. The
/// transcript ID is checked against the session before its segments enter
/// the Gemini prompt.
pub async fn run_for_source(
    db: &Db,
    gateway: &GeminiGateway,
    session_id: SessionId,
    template_id: MemoTemplateId,
    selected_transcript_id: Option<TranscriptId>,
    locale: &str,
    consent: ConsentSnapshot,
    cancellation: CancellationToken,
) -> Result<GenerateOutcome, AppError> {
    let locale = Locale::parse(locale)?;
    let inputs = capture_inputs(db, session_id, template_id, selected_transcript_id)?;

    let transcript_block = format_transcript_block(
        &inputs.segments,
        inputs.transcript_status,
        inputs.timestamp_offset_sec,
        locale,
    );
    let prompt = build_prompt(
        &inputs.template_prompt,
        &transcript_block,
        inputs.notes_body.as_deref(),
        locale,
    );
    let (path, request_body) = build_request(&inputs.model, &prompt)?;

    let response = gateway
        .post_job_observed_for(
            ModelKind::Memo,
            &path,
            request_body,
            consent,
            cancellation.clone(),
            None,
        )
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            // A cancellation racing the transport can surface as any error
            // shape depending on exactly where it landed (timeout, a typed
            // `Code::Blocked` from the gateway's own cancellation path, or a
            // transport failure) -- checking the token itself after the
            // `await` is the only reliable signal, so cancellation always
            // reports as `Cancelled` (never a scary inline error) while a
            // real failure (e.g. an actual 451) still propagates.
            if cancellation.is_cancelled() {
                return Ok(GenerateOutcome::Cancelled);
            }
            return Err(error);
        }
    };
    if cancellation.is_cancelled() {
        return Ok(GenerateOutcome::Cancelled);
    }

    let markdown = parse_memo_response(&response.body)?;
    let created_at = now_ms();
    let body = Sensitive::new(markdown.clone());
    let template_prompt = Sensitive::new(inputs.template_prompt.clone());
    let outcome = db.with_connection(|conn| {
        Ok(repo::memos::upsert(
            conn,
            session_id,
            template_id,
            &body,
            created_at,
            inputs.transcript_id,
            inputs.transcript_status.as_str(),
            inputs.notes_revision,
            &inputs.template_name,
            &template_prompt,
            &inputs.model,
        )?)
    })?;

    match outcome {
        repo::memos::UpsertOutcome::SessionGone => Ok(GenerateOutcome::Cancelled),
        repo::memos::UpsertOutcome::Saved => Ok(GenerateOutcome::Generated(MemoView {
            body: markdown,
            created_at: created_at as f64,
            transcript_status: inputs.transcript_status.as_str().to_string(),
            model: inputs.model,
            template_name: inputs.template_name,
            uses_notes: inputs.template_prompt.contains("{notes}"),
            from_previous_transcript: false,
            notes_changed: false,
        })),
    }
}

/// Đọc memo đã cache, không bao giờ gọi Gemini (spec Boundaries Always).
/// `None` khi chưa từng sinh cho cặp (Phiên, Template) này.
pub fn view(
    db: &Db,
    session_id: SessionId,
    template_id: MemoTemplateId,
) -> Result<Option<MemoView>, AppError> {
    view_for_source(db, session_id, template_id, None)
}

pub fn view_for_source(
    db: &Db,
    session_id: SessionId,
    template_id: MemoTemplateId,
    selected_transcript_id: Option<TranscriptId>,
) -> Result<Option<MemoView>, AppError> {
    db.with_connection(|conn| {
        let Some(row) = repo::memos::get(conn, session_id, template_id)? else {
            return Ok(None);
        };
        let current_transcript_id = match selected_transcript_id {
            Some(id) => Some(id),
            None => repo::transcripts::primary_for_session(conn, session_id)?,
        };
        let current_notes_revision = repo::notes::get(conn, session_id)?.map(|row| row.revision);
        Ok(Some(MemoView {
            body: row.body.into_inner(),
            created_at: row.created_at as f64,
            transcript_status: row.transcript_status,
            model: row.model,
            template_name: row.template_name,
            uses_notes: row.template_prompt.expose().contains("{notes}"),
            from_previous_transcript: current_transcript_id != Some(row.transcript_id),
            notes_changed: current_notes_revision != row.notes_revision,
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::segments::GapReason;
    use crate::db::repo::sessions::{self, NewSession};
    use crate::gemini::test_support::{gateway_with, FakeTransport};
    use crate::gemini::{GeminiTransport, TransportResponse};
    use tempfile::tempdir;

    fn open_db() -> (tempfile::TempDir, Db) {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        (dir, db)
    }

    fn insert_session(db: &Db, id: SessionId) {
        db.with_connection(|conn| {
            Ok(sessions::insert(
                conn,
                NewSession {
                    id,
                    kind: "file",
                    title: "t",
                    source_hash: None,
                    source_name: None,
                    status: "complete",
                    recovered: false,
                    duration_sec: 60.0,
                    proxy_ext: None,
                    created_at: 0,
                    updated_at: 0,
                },
            )?)
        })
        .unwrap();
    }

    fn text_segment(idx: i64, start: f64, end: f64, text: &str) -> SegmentRow {
        SegmentRow {
            idx,
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: text.to_string(),
            speaker: None,
        }
    }

    fn gap_segment(idx: i64, start: f64, end: f64, reason: GapReason) -> SegmentRow {
        SegmentRow {
            idx,
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Gap,
            gap_reason: Some(reason),
            text: String::new(),
            speaker: None,
        }
    }

    #[test]
    fn format_transcript_block_renders_text_lines_and_gap_notices() {
        let segments = vec![
            text_segment(0, 0.0, 2.0, "Xin chào"),
            gap_segment(1, 300.0, 360.0, GapReason::ChunkFailed),
        ];
        let block = format_transcript_block(&segments, TranscriptStatus::Partial, 0.0, Locale::Vi);
        assert!(block.starts_with(
            "Bản ghi bị thiếu nội dung ở các khoảng: 05:00–06:00.\n\n[00:00] Xin chào"
        ));
        assert!(block.contains("[05:00–06:00] (Thiếu nội dung ở khoảng này)"));
    }

    #[test]
    fn format_transcript_block_omits_the_partial_prefix_when_complete() {
        let segments = vec![text_segment(0, 0.0, 2.0, "Xin chào")];
        let block = format_transcript_block(&segments, TranscriptStatus::Complete, 0.0, Locale::Vi);
        assert_eq!(block, "[00:00] Xin chào");
    }

    #[test]
    fn build_prompt_replaces_notes_only_when_the_template_uses_it() {
        let with_notes = build_prompt(
            "Tóm tắt: {transcript}\n{notes}",
            "[00:00] hi",
            Some("nhớ gửi email"),
            Locale::Vi,
        );
        assert!(with_notes.contains("Ghi chú của người dùng:\nnhớ gửi email"));

        let empty_notes = build_prompt(
            "Tóm tắt: {transcript}\n{notes}",
            "[00:00] hi",
            None,
            Locale::Vi,
        );
        assert!(empty_notes.contains("(không có ghi chú)"));

        let without_notes_token = build_prompt(
            "Tóm tắt: {transcript}",
            "[00:00] hi",
            Some("bị bỏ qua"),
            Locale::Vi,
        );
        assert!(!without_notes_token.contains("bị bỏ qua"));
        assert!(!without_notes_token.contains("{notes}"));
    }

    #[test]
    fn build_request_is_text_only_with_no_generation_config() {
        let (path, body) = build_request("gemini-flash-lite-latest", "hello").unwrap();
        assert_eq!(
            path,
            "/v1beta/models/gemini-flash-lite-latest:generateContent"
        );
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            value,
            json!({"contents":[{"role":"user","parts":[{"text":"hello"}]}]})
        );
        assert!(value.get("generationConfig").is_none());
    }

    #[test]
    fn parse_memo_response_extracts_markdown_text() {
        let body =
            json!({"candidates":[{"content":{"parts":[{"text":"# Memo\n\n- a"}]}}]}).to_string();
        assert_eq!(parse_memo_response(&body).unwrap(), "# Memo\n\n- a");
    }

    #[test]
    fn parse_memo_response_reports_blocked_for_safety_or_prompt_feedback() {
        let safety =
            json!({"candidates":[{"finishReason":"SAFETY","content":{"parts":[{"text":""}]}}]})
                .to_string();
        assert_eq!(
            parse_memo_response(&safety).unwrap_err().code,
            Code::Blocked
        );

        let blocked = json!({"promptFeedback":{"blockReason":"SAFETY"}}).to_string();
        assert_eq!(
            parse_memo_response(&blocked).unwrap_err().code,
            Code::Blocked
        );
    }

    fn insert_transcript(
        db: &Db,
        session_id: SessionId,
        segments: Vec<repo::segments::SegmentDraft>,
    ) -> TranscriptId {
        let id = TranscriptId::new();
        db.with_connection(|conn| {
            Ok(repo::transcripts::insert_with_segments(
                conn,
                id,
                session_id,
                repo::transcripts::Variant::Primary,
                "gemini-flash-lite-latest",
                None,
                &segments,
                0,
            )?)
        })
        .unwrap();
        id
    }

    fn draft_text(start: f64, end: f64, text: &str) -> repo::segments::SegmentDraft {
        repo::segments::SegmentDraft {
            start_sec: start,
            end_sec: end,
            kind: SegmentKind::Text,
            gap_reason: None,
            text: text.to_string(),
            speaker: None,
        }
    }

    #[tokio::test]
    async fn run_generates_and_commits_on_success() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let transcript_id =
            insert_transcript(&db, session_id, vec![draft_text(0.0, 2.0, "Xin chào")]);
        let template = crate::memo::templates::create(
            &db,
            "Mẫu".to_string(),
            "Tóm tắt: {transcript}".to_string(),
        )
        .unwrap();

        let transport = FakeTransport::new(vec![Ok(TransportResponse {
            status: 200,
            body: json!({"candidates":[{"content":{"parts":[{"text":"# Memo"}]}}]}).to_string(),
        })]);
        let gateway = crate::gemini::test_support::gateway_with(
            vec![crate::gemini::test_support::key("a", "AIzaA123456789")],
            transport.clone(),
        )
        .await;

        let outcome = run(
            &db,
            &gateway,
            session_id,
            template.id,
            "vi",
            ConsentSnapshot::new(1, false),
            CancellationToken::new(),
        )
        .await
        .unwrap();

        let GenerateOutcome::Generated(view) = outcome else {
            panic!("expected Generated");
        };
        assert_eq!(view.body, "# Memo");
        assert!(!view.from_previous_transcript);
        assert!(!view.notes_changed);

        let cached = view_after(&db, session_id, template.id);
        assert_eq!(cached.body, "# Memo");
        let _ = transcript_id;
    }

    #[tokio::test]
    async fn run_for_source_builds_the_prompt_from_the_selected_transcript() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let primary_id =
            insert_transcript(&db, session_id, vec![draft_text(0.0, 2.0, "bản live")]);
        let retranscribe_id = TranscriptId::new();
        db.with_connection(|conn| {
            Ok(repo::transcripts::insert_with_segments(
                conn,
                retranscribe_id,
                session_id,
                repo::transcripts::Variant::Retranscribe,
                "gemini-flash-lite-latest",
                None,
                &[draft_text(0.0, 2.0, "bản từ recording")],
                1,
            )?)
        })
        .unwrap();
        let template = crate::memo::templates::create(
            &db,
            "Mẫu".to_string(),
            "Tóm tắt: {transcript}".to_string(),
        )
        .unwrap();
        let transport = FakeTransport::new(vec![Ok(TransportResponse {
            status: 200,
            body: json!({"candidates":[{"content":{"parts":[{"text":"# Memo"}]}}]}).to_string(),
        })]);
        let gateway = gateway_with(
            vec![crate::gemini::test_support::key("a", "AIzaA123456789")],
            transport.clone(),
        )
        .await;

        let outcome = run_for_source(
            &db,
            &gateway,
            session_id,
            template.id,
            Some(retranscribe_id),
            "vi",
            ConsentSnapshot::new(1, false),
            CancellationToken::new(),
        )
        .await
        .unwrap();

        assert!(matches!(outcome, GenerateOutcome::Generated(_)));
        let request = transport.requests().pop().unwrap();
        let body: Value = serde_json::from_str(request.body.as_ref().unwrap().expose()).unwrap();
        let prompt = body.pointer("/contents/0/parts/0/text").unwrap().as_str().unwrap();
        assert!(prompt.contains("[00:00] bản từ recording"));
        assert!(!prompt.contains("bản live"));
        assert_eq!(view(&db, session_id, template.id).unwrap().unwrap().from_previous_transcript, true);
        assert_ne!(primary_id, retranscribe_id);
    }

    fn view_after(db: &Db, session_id: SessionId, template_id: MemoTemplateId) -> MemoView {
        view(db, session_id, template_id).unwrap().unwrap()
    }

    #[tokio::test]
    async fn run_rejects_a_transcript_that_is_only_gaps() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        db.with_connection(|conn| {
            let id = TranscriptId::new();
            Ok(repo::transcripts::insert_with_segments(
                conn,
                id,
                session_id,
                repo::transcripts::Variant::Primary,
                "m",
                None,
                &[repo::segments::SegmentDraft {
                    start_sec: 0.0,
                    end_sec: 1.0,
                    kind: SegmentKind::Gap,
                    gap_reason: Some(GapReason::ChunkFailed),
                    text: String::new(),
                    speaker: None,
                }],
                0,
            )?)
        })
        .unwrap();
        let template =
            crate::memo::templates::create(&db, "Mẫu".to_string(), "{transcript}".to_string())
                .unwrap();
        let transport = FakeTransport::new(vec![]);
        let gateway = gateway_with(
            vec![crate::gemini::test_support::key("a", "AIzaA123456789")],
            transport.clone(),
        )
        .await;

        let error = run(
            &db,
            &gateway,
            session_id,
            template.id,
            "vi",
            ConsentSnapshot::new(1, false),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Request);
        assert!(transport.requests().is_empty());
    }

    #[tokio::test]
    async fn run_keeps_the_old_memo_when_gemini_errors() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        insert_transcript(&db, session_id, vec![draft_text(0.0, 2.0, "Xin chào")]);
        let template =
            crate::memo::templates::create(&db, "Mẫu".to_string(), "{transcript}".to_string())
                .unwrap();

        // 400 is non-retryable (unlike 429/5xx) so a single fixture response
        // is enough — matches `gemini::mod::post_job_rejects_nonretryable_http_errors_and_oversized_body`.
        let transport = FakeTransport::new(vec![Ok(TransportResponse {
            status: 400,
            body: "rejected".to_string(),
        })]);
        let gateway = gateway_with(
            vec![crate::gemini::test_support::key("a", "AIzaA123456789")],
            transport,
        )
        .await;

        let error = run(
            &db,
            &gateway,
            session_id,
            template.id,
            "vi",
            ConsentSnapshot::new(1, false),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, Code::Request);
        assert_eq!(view(&db, session_id, template.id).unwrap(), None);
    }

    #[tokio::test]
    async fn run_reports_cancelled_and_writes_nothing_when_cancelled_before_completion() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        insert_transcript(&db, session_id, vec![draft_text(0.0, 2.0, "Xin chào")]);
        let template =
            crate::memo::templates::create(&db, "Mẫu".to_string(), "{transcript}".to_string())
                .unwrap();

        struct PendingTransport;
        impl GeminiTransport for PendingTransport {
            fn send(
                &self,
                _request: crate::gemini::TransportRequest,
            ) -> crate::gemini::TransportFuture {
                Box::pin(std::future::pending())
            }
        }
        let gateway = crate::gemini::test_support::gateway_with(
            vec![crate::gemini::test_support::key("a", "AIzaA123456789")],
            std::sync::Arc::new(PendingTransport),
        )
        .await;

        let cancellation = CancellationToken::new();
        let cancel_clone = cancellation.clone();
        let db_ref = &db;
        let gateway_ref = &gateway;
        let operation = async {
            run(
                db_ref,
                gateway_ref,
                session_id,
                template.id,
                "vi",
                ConsentSnapshot::new(1, false),
                cancellation,
            )
            .await
        };
        let canceller = async {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            cancel_clone.cancel();
        };
        let (outcome, ()) = tokio::join!(operation, canceller);
        assert_eq!(outcome.unwrap(), GenerateOutcome::Cancelled);
        assert_eq!(view(&db, session_id, template.id).unwrap(), None);
    }

    #[test]
    fn view_reports_previous_transcript_and_notes_changed() {
        let (_dir, db) = open_db();
        let session_id = SessionId::new();
        insert_session(&db, session_id);
        let old_transcript_id =
            insert_transcript(&db, session_id, vec![draft_text(0.0, 2.0, "cũ")]);
        let template =
            crate::memo::templates::create(&db, "Mẫu".to_string(), "{transcript}".to_string())
                .unwrap();

        let body = Sensitive::new("nội dung".to_string());
        let prompt = Sensitive::new(template.prompt.clone());
        db.with_connection(|conn| {
            Ok(repo::memos::upsert(
                conn,
                session_id,
                template.id,
                &body,
                0,
                old_transcript_id,
                "complete",
                Some(1),
                &template.name,
                &prompt,
                "m",
            )?)
        })
        .unwrap();

        // Chưa đổi gì -- cả hai cờ đều false.
        let fresh = view_after(&db, session_id, template.id);
        assert!(!fresh.from_previous_transcript);
        assert!(fresh.notes_changed, "revision hiện tại None != Some(1)");

        // Chạy lại transcript (xoá primary cũ, tạo mới) -- `fromPreviousTranscript`.
        db.with_connection(|conn| {
            repo::transcripts::replace_primary(
                conn,
                session_id,
                TranscriptId::new(),
                "m",
                None,
                &[],
                1,
            )?;
            Ok(())
        })
        .unwrap();
        let after_rerun = view_after(&db, session_id, template.id);
        assert!(after_rerun.from_previous_transcript);
    }
}
