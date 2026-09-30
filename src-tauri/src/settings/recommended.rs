//! On-demand, signed recommended settings preview and transactional apply.
//!
//! The remote payload is intentionally narrow: unknown JSON fields are
//! ignored, while only four settings and the two built-in memo template IDs
//! can become a displayed/applicable change.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;

use crate::core::error::{AppError, Code};
use crate::core::id::MemoTemplateId;
use crate::db::{repo, Db};
use crate::memo::{defaults, templates};
use crate::remote::RemoteDocument;

const SCHEMA_VERSION: u32 = 1;
const ALLOWED_TEMPLATE_IDS: [&str; 2] = ["meeting-minutes", "bilingual-ja-vi"];
const ALLOWED_LOCALES: [&str; 3] = ["vi", "en", "ja"];

/// Payload inside the Story 6.1 signed envelope. Unknown fields are ignored;
/// this lets a later publisher add unrelated fields without broadening the
/// fields this client is able to display or persist.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedSettingsDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub settings: RecommendedValues,
    #[serde(default)]
    pub templates: Vec<RecommendedTemplate>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedValues {
    pub transcribe_model: Option<String>,
    pub live_model: Option<String>,
    pub memo_model: Option<String>,
    pub chunk_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedTemplate {
    pub external_id: String,
    pub locale: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RecommendedSettingField {
    TranscribeModel,
    LiveModel,
    MemoModel,
    ChunkMinutes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedSettingChange {
    pub field: RecommendedSettingField,
    pub current_value: String,
    pub proposed_value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedTemplateChange {
    pub external_id: String,
    pub locale: String,
    pub current_name: Option<String>,
    pub proposed_name: String,
    pub current_prompt: Option<String>,
    pub proposed_prompt: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedTemplateConflict {
    pub external_id: String,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedSettingsPreview {
    /// Opaque, process-local UUID. It is invalid after cancel, apply, or app
    /// restart and grants no access beyond this one preview plan.
    pub token: String,
    pub changes: Vec<RecommendedSettingChange>,
    pub template_changes: Vec<RecommendedTemplateChange>,
    pub template_conflicts: Vec<RecommendedTemplateConflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedSettingsApplyResult {
    /// A stale result carries a newly computed preview and has written nothing.
    pub stale: bool,
    pub applied_count: u32,
    pub preview: Option<RecommendedSettingsPreview>,
}

#[derive(Default)]
pub struct RecommendedPreviewStore {
    active: Mutex<Option<StoredPreview>>,
}

#[derive(Debug, Clone)]
struct StoredPreview {
    preview: RecommendedSettingsPreview,
    payload_digest: String,
    local_revision: String,
}

#[derive(Debug, Clone)]
struct LocalSnapshot {
    settings: super::Settings,
    templates: Vec<repo::memo_templates::MemoTemplateRow>,
    template_states: BTreeMap<(String, String), (String, String)>,
    revision: String,
}

#[derive(Debug, Clone)]
enum TransactionOutcome {
    Applied { count: u32 },
    Stale(LocalSnapshot),
}

impl RecommendedPreviewStore {
    /// New previews replace the prior token. There is one visible preview in
    /// the Settings panel, and an older hidden token must not remain usable.
    fn replace(&self, value: StoredPreview) -> Result<(), AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| AppError::new(Code::Storage, "Preview state is unavailable"))?;
        *active = Some(value);
        Ok(())
    }

    fn peek(&self, token: &str) -> Result<StoredPreview, AppError> {
        self.active
            .lock()
            .map_err(|_| AppError::new(Code::Storage, "Preview state is unavailable"))?
            .as_ref()
            .filter(|stored| stored.preview.token == token)
            .cloned()
            .ok_or_else(|| AppError::new(Code::Request, "Preview expired or was cancelled"))
    }

    fn take(&self, token: &str) -> Result<StoredPreview, AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| AppError::new(Code::Storage, "Preview state is unavailable"))?;
        if active
            .as_ref()
            .is_some_and(|stored| stored.preview.token == token)
        {
            return active
                .take()
                .ok_or_else(|| AppError::new(Code::Request, "Preview expired or was cancelled"));
        }
        Err(AppError::new(
            Code::Request,
            "Preview expired or was cancelled",
        ))
    }

    /// Idempotent for an already invalidated or superseded token.
    pub fn cancel(&self, token: &str) -> Result<(), AppError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| AppError::new(Code::Storage, "Preview state is unavailable"))?;
        if active
            .as_ref()
            .is_some_and(|stored| stored.preview.token == token)
        {
            *active = None;
        }
        Ok(())
    }

    pub fn require_active(&self, token: &str) -> Result<(), AppError> {
        self.peek(token).map(|_| ())
    }
}

/// Build a preview from a fresh signed document. This only reads local state;
/// the memo template list API is deliberately avoided because it seeds rows.
pub fn preview(
    db: &Db,
    store: &RecommendedPreviewStore,
    document: RemoteDocument<RecommendedSettingsDocument>,
) -> Result<RecommendedSettingsPreview, AppError> {
    if !matches!(document.provenance, crate::remote::RemoteProvenance::Fresh) {
        return Err(AppError::new(
            Code::Network,
            "A fresh recommendation is required",
        ));
    }
    let digest = document
        .signed_payload_digest
        .map(|digest| hex(&digest))
        .ok_or_else(|| AppError::new(Code::Format, "Recommendation signature is missing"))?;
    let document = validate_document(document.payload)?;
    let snapshot = read_snapshot(db)?;
    let stored = build_stored_preview(snapshot, document, digest)?;
    let preview = stored.preview.clone();
    store.replace(stored)?;
    Ok(preview)
}

/// Apply the exact plan shown in a preview. The payload is fetched fresh by
/// IPC for every apply click. The local revision check and both tables' writes
/// run under one SQLite transaction and one connection lock.
pub fn apply(
    db: &Db,
    store: &RecommendedPreviewStore,
    token: &str,
    document: RemoteDocument<RecommendedSettingsDocument>,
) -> Result<RecommendedSettingsApplyResult, AppError> {
    store.peek(token)?;
    if !matches!(document.provenance, crate::remote::RemoteProvenance::Fresh) {
        return Err(AppError::new(
            Code::Network,
            "A fresh recommendation is required",
        ));
    }
    let digest = document
        .signed_payload_digest
        .map(|digest| hex(&digest))
        .ok_or_else(|| AppError::new(Code::Format, "Recommendation signature is missing"))?;
    let document = validate_document(document.payload)?;
    let stored = store.take(token)?;
    let outcome = db.with_connection(|conn| {
        let tx = conn.transaction()?;
        let snapshot = read_snapshot_from(&tx)?;
        if snapshot.revision != stored.local_revision || digest != stored.payload_digest {
            tx.rollback()?;
            return Ok(TransactionOutcome::Stale(snapshot));
        }

        let mut entries = Vec::new();
        for change in &stored.preview.changes {
            let encoded = match change.field {
                RecommendedSettingField::TranscribeModel => {
                    let value = super::validate_recommended_model(
                        "transcribeModel",
                        &change.proposed_value,
                    )?;
                    if value != change.proposed_value {
                        return Err(AppError::new(
                            Code::Format,
                            "Preview value is no longer valid",
                        ));
                    }
                    serde_json::to_string(&value)
                }
                RecommendedSettingField::LiveModel => {
                    let value =
                        super::validate_recommended_model("liveModel", &change.proposed_value)?;
                    if value != change.proposed_value {
                        return Err(AppError::new(
                            Code::Format,
                            "Preview value is no longer valid",
                        ));
                    }
                    serde_json::to_string(&value)
                }
                RecommendedSettingField::MemoModel => {
                    let value =
                        super::validate_recommended_model("memoModel", &change.proposed_value)?;
                    if value != change.proposed_value {
                        return Err(AppError::new(
                            Code::Format,
                            "Preview value is no longer valid",
                        ));
                    }
                    serde_json::to_string(&value)
                }
                RecommendedSettingField::ChunkMinutes => {
                    let value = change.proposed_value.parse::<u32>().map_err(|_| {
                        AppError::new(Code::Format, "Preview contains an invalid chunk length")
                    })?;
                    super::require_min_chunk_minutes(value)?;
                    serde_json::to_string(&value)
                }
            }
            .map_err(|_| AppError::new(Code::Format, "Recommended value could not be encoded"))?;
            let key = match change.field {
                RecommendedSettingField::TranscribeModel => "transcribeModel",
                RecommendedSettingField::LiveModel => "liveModel",
                RecommendedSettingField::MemoModel => "memoModel",
                RecommendedSettingField::ChunkMinutes => "chunkMinutes",
            };
            entries.push((key, encoded));
        }

        let now = now_ms();
        repo::settings::upsert_many_in_transaction(&tx, &entries)?;
        for template in &stored.preview.template_changes {
            let (name, prompt) = templates::validate_recommended_content(
                &template.proposed_name,
                &template.proposed_prompt,
            )?;
            if name != template.proposed_name || prompt != template.proposed_prompt {
                return Err(AppError::new(
                    Code::Format,
                    "Preview template is no longer valid",
                ));
            }
            repo::memo_templates::upsert_recommended(
                &tx,
                MemoTemplateId::new(),
                &name,
                &prompt,
                &template.locale,
                &template.external_id,
                now,
            )?;
            repo::memo_templates::upsert_recommended_state(
                &tx,
                &template.locale,
                &template.external_id,
                &name,
                &prompt,
                now,
            )?;
        }
        tx.commit()?;
        let count = (stored.preview.changes.len() + stored.preview.template_changes.len()) as u32;
        let _ = snapshot.settings;
        Ok(TransactionOutcome::Applied { count })
    })?;

    match outcome {
        TransactionOutcome::Applied { count } => Ok(RecommendedSettingsApplyResult {
            stale: false,
            applied_count: count,
            preview: None,
        }),
        TransactionOutcome::Stale(snapshot) => {
            let stored = build_stored_preview(snapshot, document, digest)?;
            let preview = stored.preview.clone();
            store.replace(stored)?;
            Ok(RecommendedSettingsApplyResult {
                stale: true,
                applied_count: 0,
                preview: Some(preview),
            })
        }
    }
}

fn validate_document(
    mut document: RecommendedSettingsDocument,
) -> Result<RecommendedSettingsDocument, AppError> {
    if document.schema_version != SCHEMA_VERSION {
        return Err(AppError::new(
            Code::Format,
            "Recommended settings schema version is unsupported",
        ));
    }
    if let Some(value) = document.settings.transcribe_model.take() {
        document.settings.transcribe_model = Some(super::validate_recommended_model(
            "transcribeModel",
            &value,
        )?);
    }
    if let Some(value) = document.settings.live_model.take() {
        document.settings.live_model =
            Some(super::validate_recommended_model("liveModel", &value)?);
    }
    if let Some(value) = document.settings.memo_model.take() {
        document.settings.memo_model =
            Some(super::validate_recommended_model("memoModel", &value)?);
    }
    if let Some(value) = document.settings.chunk_minutes {
        super::require_min_chunk_minutes(value)?;
    }
    if document.templates.len() > ALLOWED_TEMPLATE_IDS.len() * ALLOWED_LOCALES.len() {
        return Err(AppError::new(
            Code::Format,
            "Too many recommended templates",
        ));
    }

    let mut seen = HashSet::new();
    for template in &mut document.templates {
        if !ALLOWED_TEMPLATE_IDS.contains(&template.external_id.as_str())
            || !ALLOWED_LOCALES.contains(&template.locale.as_str())
            || !seen.insert((template.locale.clone(), template.external_id.clone()))
        {
            return Err(AppError::new(
                Code::Format,
                "Recommended template identifier is unsupported",
            ));
        }
        let (name, prompt) =
            templates::validate_recommended_content(&template.name, &template.prompt)?;
        template.name = name;
        template.prompt = prompt;
    }

    Ok(document)
}

fn read_snapshot(db: &Db) -> Result<LocalSnapshot, AppError> {
    db.with_connection(|conn| read_snapshot_from(conn))
}

fn read_snapshot_from(conn: &rusqlite::Connection) -> Result<LocalSnapshot, AppError> {
    let raw = repo::settings::read_all(conn)?;
    let settings = super::load_from_raw(&raw);
    let templates = repo::memo_templates::list_all(conn)?;
    let template_states = repo::memo_templates::list_recommended_states(conn)?
        .into_iter()
        .map(|(locale, external_id, name, prompt)| ((locale, external_id), (name, prompt)))
        .collect::<BTreeMap<_, _>>();
    let serialized_templates: Vec<_> = templates
        .iter()
        .map(|row| {
            (
                row.id.to_string(),
                row.name.as_str(),
                row.prompt.as_str(),
                row.is_default,
                row.locale.as_deref(),
                row.default_key.as_deref(),
                row.created_at,
                row.updated_at,
            )
        })
        .collect();
    let serialized_template_states: Vec<_> = template_states
        .iter()
        .map(|((locale, external_id), (name, prompt))| (locale, external_id, name, prompt))
        .collect();
    let bytes = serde_json::to_vec(&(
        BTreeMap::from_iter(raw),
        serialized_templates,
        serialized_template_states,
    ))
    .map_err(|_| AppError::new(Code::Storage, "Settings revision could not be computed"))?;
    let revision = hex(&Sha256::digest(bytes));
    Ok(LocalSnapshot {
        settings,
        templates,
        template_states,
        revision,
    })
}

fn build_stored_preview(
    snapshot: LocalSnapshot,
    document: RecommendedSettingsDocument,
    payload_digest: String,
) -> Result<StoredPreview, AppError> {
    let mut changes = Vec::new();
    if let Some(proposed_value) = document.settings.transcribe_model {
        push_setting_change(
            &mut changes,
            RecommendedSettingField::TranscribeModel,
            snapshot.settings.transcribe_model,
            proposed_value,
        );
    }
    if let Some(proposed_value) = document.settings.live_model {
        push_setting_change(
            &mut changes,
            RecommendedSettingField::LiveModel,
            snapshot.settings.live_model,
            proposed_value,
        );
    }
    if let Some(proposed_value) = document.settings.memo_model {
        push_setting_change(
            &mut changes,
            RecommendedSettingField::MemoModel,
            snapshot.settings.memo_model,
            proposed_value,
        );
    }
    if let Some(proposed_value) = document.settings.chunk_minutes {
        push_setting_change(
            &mut changes,
            RecommendedSettingField::ChunkMinutes,
            snapshot.settings.chunk_minutes.to_string(),
            proposed_value.to_string(),
        );
    }

    let mut template_changes = Vec::new();
    let mut template_conflicts = Vec::new();
    for candidate in document.templates {
        let current = snapshot.templates.iter().find(|row| {
            row.is_default
                && row.locale.as_deref() == Some(candidate.locale.as_str())
                && row.default_key.as_deref() == Some(candidate.external_id.as_str())
        });
        if current.is_some_and(|row| row.name == candidate.name && row.prompt == candidate.prompt) {
            continue;
        }

        if current
            .is_some_and(|row| !template_is_untouched(row, &candidate, &snapshot.template_states))
        {
            template_conflicts.push(RecommendedTemplateConflict {
                external_id: candidate.external_id,
                locale: candidate.locale,
            });
            continue;
        }

        template_changes.push(RecommendedTemplateChange {
            external_id: candidate.external_id,
            locale: candidate.locale,
            current_name: current.map(|row| row.name.clone()),
            proposed_name: candidate.name,
            current_prompt: current.map(|row| row.prompt.clone()),
            proposed_prompt: candidate.prompt,
        });
    }

    let token = uuid::Uuid::now_v7().to_string();
    let preview = RecommendedSettingsPreview {
        token,
        changes,
        template_changes,
        template_conflicts,
    };
    Ok(StoredPreview {
        preview,
        payload_digest,
        local_revision: snapshot.revision,
    })
}

fn template_is_untouched(
    row: &repo::memo_templates::MemoTemplateRow,
    candidate: &RecommendedTemplate,
    template_states: &BTreeMap<(String, String), (String, String)>,
) -> bool {
    if let Some((name, prompt)) =
        template_states.get(&(candidate.locale.clone(), candidate.external_id.clone()))
    {
        return row.name == *name && row.prompt == *prompt;
    }
    // Before a remote baseline exists, only the built-in default text proves
    // that the local copy has not been edited.
    defaults::defaults_for_locale(&candidate.locale)
        .into_iter()
        .flatten()
        .find(|default| default.default_key == candidate.external_id)
        .is_some_and(|default| row.name == default.name && row.prompt == default.prompt)
}

fn push_setting_change(
    changes: &mut Vec<RecommendedSettingChange>,
    field: RecommendedSettingField,
    current_value: String,
    proposed_value: String,
) {
    if current_value != proposed_value {
        changes.push(RecommendedSettingChange {
            field,
            current_value,
            proposed_value,
        });
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::Category;
    use crate::db::repo::memo_templates;
    use crate::remote::test_fresh_document;
    use tempfile::tempdir;

    fn open_db() -> (tempfile::TempDir, Db) {
        let dir = tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        (dir, db)
    }

    fn document(
        transcribe_model: Option<&str>,
        chunk_minutes: Option<u32>,
    ) -> RecommendedSettingsDocument {
        RecommendedSettingsDocument {
            schema_version: 1,
            settings: RecommendedValues {
                transcribe_model: transcribe_model.map(str::to_owned),
                chunk_minutes,
                ..RecommendedValues::default()
            },
            templates: Vec::new(),
        }
    }

    fn preview_document(
        db: &Db,
        store: &RecommendedPreviewStore,
        document: RecommendedSettingsDocument,
    ) -> RecommendedSettingsPreview {
        preview(db, store, test_fresh_document(document)).unwrap()
    }

    #[test]
    fn preview_ignores_unknown_key_and_consent_fields_and_performs_no_writes() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        let payload: serde_json::Value = serde_json::json!({
            "schemaVersion": 1,
            "settings": {
                "transcribeModel": "models/gemini-recommended",
                "apiKey": "must-not-be-retained",
                "consentAcceptedVersion": 999
            },
            "apiKey": "also-ignored",
            "consent": {"accepted": true},
            "unknownFutureField": "ignored"
        });
        let document: RecommendedSettingsDocument = serde_json::from_value(payload).unwrap();

        let before = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        let preview = preview_document(&db, &store, document);
        let after = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        let rendered = serde_json::to_string(&preview).unwrap();

        assert_eq!(before, after);
        assert_eq!(preview.changes.len(), 1);
        assert_eq!(
            preview.changes[0].field,
            RecommendedSettingField::TranscribeModel
        );
        assert_eq!(preview.changes[0].proposed_value, "gemini-recommended");
        assert!(!rendered.contains("apiKey"));
        assert!(!rendered.contains("consent"));
        assert!(!rendered.contains("must-not-be-retained"));
    }

    #[test]
    fn unsupported_schema_and_out_of_range_chunk_are_redacted_errors_without_writes() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        let before = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();

        let mut future = document(None, None);
        future.schema_version = 2;
        let future_error = preview(&db, &store, test_fresh_document(future)).unwrap_err();
        let range_error =
            preview(&db, &store, test_fresh_document(document(None, Some(61)))).unwrap_err();

        assert_eq!(future_error.category, Category::Format);
        assert_eq!(range_error.category, Category::Format);
        let after = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn apply_writes_only_displayed_allowlisted_settings() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        db.with_connection(|conn| {
            repo::settings::upsert_many(
                conn,
                &[
                    ("theme", "\"dark\"".to_owned()),
                    ("consentAcceptedVersion", "7".to_owned()),
                    ("memoModel", "\"memo-local\"".to_owned()),
                ],
            )?;
            Ok(())
        })
        .unwrap();
        let remote = document(Some("gemini-transcribe-new"), Some(8));
        let shown = preview_document(&db, &store, remote.clone());
        let result = apply(&db, &store, &shown.token, test_fresh_document(remote)).unwrap();

        assert!(!result.stale);
        assert_eq!(result.applied_count, 2);
        assert!(result.preview.is_none());
        let raw = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        assert_eq!(raw.get("theme").map(String::as_str), Some("\"dark\""));
        assert_eq!(
            raw.get("consentAcceptedVersion").map(String::as_str),
            Some("7")
        );
        assert_eq!(
            raw.get("memoModel").map(String::as_str),
            Some("\"memo-local\"")
        );
        assert_eq!(
            raw.get("transcribeModel").map(String::as_str),
            Some("\"gemini-transcribe-new\"")
        );
        assert_eq!(raw.get("chunkMinutes").map(String::as_str), Some("8"));
    }

    #[test]
    fn stale_local_revision_returns_a_new_diff_without_applying() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        let remote = document(Some("gemini-transcribe-new"), None);
        let shown = preview_document(&db, &store, remote.clone());
        db.with_connection(|conn| {
            repo::settings::upsert_many(conn, &[("theme", "\"dark\"".to_owned())])?;
            Ok(())
        })
        .unwrap();

        let result = apply(&db, &store, &shown.token, test_fresh_document(remote)).unwrap();

        assert!(result.stale);
        assert_eq!(result.applied_count, 0);
        let fresh = result
            .preview
            .expect("stale result carries a refreshed preview");
        assert_ne!(fresh.token, shown.token);
        assert_eq!(fresh.changes.len(), 1);
        let raw = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        assert!(raw.get("transcribeModel").is_none());
        assert_eq!(raw.get("theme").map(String::as_str), Some("\"dark\""));
    }

    #[test]
    fn changed_payload_digest_refreshes_the_preview_without_writing() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        let original = document(Some("model-a"), None);
        let shown = preview_document(&db, &store, original);
        let changed = document(Some("model-b"), None);

        let result = apply(&db, &store, &shown.token, test_fresh_document(changed)).unwrap();

        assert!(result.stale);
        let fresh = result.preview.unwrap();
        assert_ne!(fresh.token, shown.token);
        assert_eq!(fresh.changes[0].proposed_value, "model-b");
        let raw = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        assert!(raw.get("transcribeModel").is_none());
    }

    #[test]
    fn cancel_invalidates_the_token_and_writes_nothing() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        let remote = document(Some("model-a"), None);
        let shown = preview_document(&db, &store, remote.clone());
        store.cancel(&shown.token).unwrap();

        let error = apply(&db, &store, &shown.token, test_fresh_document(remote)).unwrap_err();

        assert_eq!(error.code, Code::Request);
        let raw = db
            .with_connection(|conn| Ok(repo::settings::read_all(conn)?))
            .unwrap();
        assert!(raw.get("transcribeModel").is_none());
    }

    #[test]
    fn edited_recommended_template_is_reported_as_conflict_and_preserved() {
        let (_dir, db) = open_db();
        let store = RecommendedPreviewStore::default();
        let builtin = defaults::defaults_for_locale("en").unwrap()[0];
        db.with_connection(|conn| {
            memo_templates::ensure_default(
                conn,
                MemoTemplateId::new(),
                builtin.name,
                builtin.prompt,
                "en",
                builtin.default_key,
                1,
            )?;
            Ok(())
        })
        .unwrap();

        let recommended = RecommendedTemplate {
            external_id: "meeting-minutes".to_owned(),
            locale: "en".to_owned(),
            name: "Recommended minutes".to_owned(),
            prompt: "Summarize faithfully: {transcript}".to_owned(),
        };
        let first_document = RecommendedSettingsDocument {
            schema_version: 1,
            settings: RecommendedValues {
                transcribe_model: Some("model-v2".to_owned()),
                ..RecommendedValues::default()
            },
            templates: vec![recommended.clone()],
        };
        let first = preview_document(&db, &store, first_document.clone());
        assert_eq!(first.template_changes.len(), 1);
        let first_result = apply(
            &db,
            &store,
            &first.token,
            test_fresh_document(first_document),
        )
        .unwrap();
        assert_eq!(first_result.applied_count, 2);

        let row = db
            .with_connection(|conn| {
                memo_templates::get_recommended(conn, "en", "meeting-minutes").map_err(Into::into)
            })
            .unwrap()
            .unwrap();
        db.with_connection(|conn| {
            memo_templates::update_name_prompt(
                conn,
                row.id,
                "Local edit",
                "Keep this prompt: {transcript}",
                3,
            )?;
            Ok(())
        })
        .unwrap();

        let mut later_template = recommended;
        later_template.name = "New remote version".to_owned();
        later_template.prompt = "New signed prompt: {transcript}".to_owned();
        let later_document = RecommendedSettingsDocument {
            schema_version: 1,
            settings: RecommendedValues {
                transcribe_model: Some("model-v3".to_owned()),
                ..RecommendedValues::default()
            },
            templates: vec![later_template],
        };
        let later = preview_document(&db, &store, later_document.clone());
        assert!(later.template_changes.is_empty());
        assert_eq!(later.template_conflicts.len(), 1);
        let later_result = apply(
            &db,
            &store,
            &later.token,
            test_fresh_document(later_document),
        )
        .unwrap();
        assert_eq!(later_result.applied_count, 1);

        let row = db
            .with_connection(|conn| {
                memo_templates::get_recommended(conn, "en", "meeting-minutes").map_err(Into::into)
            })
            .unwrap()
            .unwrap();
        assert_eq!(row.name, "Local edit");
        assert_eq!(row.prompt, "Keep this prompt: {transcript}");
        let settings = super::super::load(&db);
        assert_eq!(settings.transcribe_model, "model-v3");
    }
}
