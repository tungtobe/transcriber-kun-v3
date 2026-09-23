//! API-key storage owned by the operating system.
//!
//! The credential store contains one versioned JSON payload. The payload is
//! deliberately kept as a single credential rather than a plaintext
//! manifest: an ID and its secret are always written together to Keychain or
//! Credential Manager. Everything that crosses this module's public boundary
//! is either [`Sensitive<String>`] or redacted metadata.

use std::fmt;
use std::sync::{Arc, Mutex};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use specta::Type;
use uuid::Uuid;

use crate::core::error::{AppError, Code};
use crate::core::sensitive::Sensitive;

/// Stable service name used by both supported native credential stores.
pub const SERVICE_NAME: &str = "com.transkun.app";
/// The username identifies the versioned key-pool payload within the service.
pub const CREDENTIAL_NAME: &str = "gemini-api-keys";
const PAYLOAD_VERSION: u8 = 1;

/// An opaque identifier for one stored API key. The secret is never part of
/// this value; IDs are UUIDv7 so they are safe to pass through IPC and logs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct KeyId(String);

impl KeyId {
    /// Create a new opaque ID for a newly stored key.
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// Construct an ID while restoring a credential payload or in a fake
    /// provider test. Production writes only use [`Self::new`].
    pub fn from_opaque(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_persisted_uuidv7(&self) -> bool {
        Uuid::parse_str(&self.0)
            .map(|value| value.get_version_num() == 7)
            .unwrap_or(false)
    }
}

impl Default for KeyId {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata safe to return to the frontend. `label` is intentionally masked;
/// neither this type nor its serialized representation contains the
/// credential itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyMetadata {
    pub id: KeyId,
    pub label: String,
}

/// A key held in RAM by the Gemini port. The secret remains wrapped even
/// while the request is in flight so accidental `Debug`/`Display` logging is
/// harmless.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyMaterial {
    pub id: KeyId,
    pub secret: Sensitive<String>,
}

impl fmt::Debug for KeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyMaterial")
            .field("id", &self.id)
            .field("secret", &self.secret)
            .finish()
    }
}

impl KeyMaterial {
    pub fn new(id: KeyId, secret: impl Into<String>) -> Self {
        Self {
            id,
            secret: Sensitive::new(secret.into()),
        }
    }

    pub fn generated(secret: impl Into<String>) -> Self {
        Self::new(KeyId::new(), secret)
    }

    pub fn metadata(&self) -> KeyMetadata {
        KeyMetadata {
            id: self.id.clone(),
            label: masked_label(self.secret.expose()),
        }
    }
}

/// Normalize and validate a comma-separated key field.
///
/// Empty fields are ignored, surrounding whitespace is removed, and exact
/// duplicate secrets are retained only once. Any non-empty item that does not
/// use one of the two accepted Gemini prefixes rejects the entire input before
/// the store is touched.
pub fn normalize_api_keys(input: &str) -> Result<Vec<Sensitive<String>>, AppError> {
    let mut values = Vec::new();

    for item in input
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if !is_supported_key(item) {
            return Err(AppError::new(
                Code::Format,
                "one or more API keys use an unsupported format",
            ));
        }

        // Keep values wrapped even in the deduplication pass. This avoids a
        // second raw-secret collection in the normalizer.
        let value = Sensitive::new(item.to_owned());
        if !values.iter().any(|existing| existing == &value) {
            values.push(value);
        }
    }

    Ok(values)
}

fn is_supported_key(value: &str) -> bool {
    (value.starts_with("AIza") && value.len() > 4) || (value.starts_with("AQ.") && value.len() > 3)
}

/// Return an opaque, human-readable label without exposing the key body.
pub fn masked_label(secret: &str) -> String {
    let prefix = if secret.starts_with("AQ.") {
        "AQ."
    } else {
        "AIza"
    };
    // Keep at least four characters hidden even for format-valid short keys.
    if secret.chars().count() < prefix.chars().count() + 8 {
        return format!("{prefix}••••");
    }
    let suffix: String = secret
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("{prefix}••••{suffix}")
}

/// The smallest backend needed by the service. Implementations must keep the
/// payload in the native credential store; they must not copy it to a file,
/// database, log, or event.
pub trait CredentialStore: Send + Sync + 'static {
    fn read(&self) -> Result<Option<Sensitive<String>>, StoreError>;
    fn write(&self, payload: &Sensitive<String>) -> Result<(), StoreError>;
    fn delete(&self) -> Result<(), StoreError>;
}

/// Deliberately detail-free store error. In particular, backend error text is
/// not carried into an `AppError`, where it could contain a secret or a
/// platform-specific path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    Unavailable,
    Failed,
    Corrupt,
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => {
                "credential store unavailable; retry or check OS credential access"
            }
            Self::Failed => {
                "credential store operation failed; retry or check OS credential access"
            }
            Self::Corrupt => "credential store payload is invalid; replace the stored keys",
        })
    }
}

impl std::error::Error for StoreError {}

fn storage_error(error: StoreError) -> AppError {
    AppError::new(Code::Storage, error.to_string())
}

/// Native Keychain (macOS) / Credential Manager (Windows) adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeCredentialStore;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl NativeCredentialStore {
    fn entry(&self) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(SERVICE_NAME, CREDENTIAL_NAME).map_err(|_| StoreError::Unavailable)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl CredentialStore for NativeCredentialStore {
    fn read(&self) -> Result<Option<Sensitive<String>>, StoreError> {
        match self.entry()?.get_password() {
            Ok(value) => Ok(Some(Sensitive::new(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(StoreError::Failed),
        }
    }

    fn write(&self, payload: &Sensitive<String>) -> Result<(), StoreError> {
        self.entry()?
            .set_password(payload.expose())
            .map_err(|_| StoreError::Failed)
    }

    fn delete(&self) -> Result<(), StoreError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(StoreError::Failed),
        }
    }
}

// Linux is intentionally not a supported product target. Keeping an
// explicit unavailable adapter lets CI compile and run parser/actor tests
// without ever opting into Secret Service or persisting a credential there.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl CredentialStore for NativeCredentialStore {
    fn read(&self) -> Result<Option<Sensitive<String>>, StoreError> {
        Err(StoreError::Unavailable)
    }

    fn write(&self, _payload: &Sensitive<String>) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }

    fn delete(&self) -> Result<(), StoreError> {
        Err(StoreError::Unavailable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredPayload {
    version: u8,
    keys: Vec<StoredKey>,
}

/// Custom serialization keeps the secret wrapped at every Rust boundary. A
/// serializer receives the value only through `Sensitive::expose()` while
/// producing the credential payload that is immediately wrapped again before
/// being handed to the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredKey {
    id: KeyId,
    secret: Sensitive<String>,
}

impl Serialize for StoredKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire<'a> {
            id: &'a KeyId,
            secret: &'a str,
        }

        Wire {
            id: &self.id,
            secret: self.secret.expose(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StoredKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            id: KeyId,
            secret: String,
        }

        let wire = Wire::deserialize(deserializer)?;
        if !wire.id.is_persisted_uuidv7() || !is_supported_key(&wire.secret) {
            return Err(D::Error::custom("stored key has unsupported format"));
        }
        Ok(Self {
            id: wire.id,
            secret: Sensitive::new(wire.secret),
        })
    }
}

/// Key service with serialized read/modify/write operations. The service
/// mutex makes set/list/delete atomic as a unit; the actor KeyPool below owns
/// its own state and does not use shared locking.
pub struct SecretService<B> {
    backend: B,
    operation: Mutex<()>,
}

impl<B> SecretService<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            operation: Mutex::new(()),
        }
    }
}

impl<B: CredentialStore> SecretService<B> {
    fn load_payload(&self) -> Result<StoredPayload, AppError> {
        let value = self.backend.read().map_err(storage_error)?;
        let Some(value) = value else {
            return Ok(StoredPayload {
                version: PAYLOAD_VERSION,
                keys: Vec::new(),
            });
        };

        let payload: StoredPayload =
            serde_json::from_str(value.expose()).map_err(|_| storage_error(StoreError::Corrupt))?;
        if payload.version != PAYLOAD_VERSION || has_duplicate_ids_or_secrets(&payload.keys) {
            return Err(storage_error(StoreError::Corrupt));
        }
        Ok(payload)
    }

    fn save_payload(&self, payload: StoredPayload) -> Result<(), AppError> {
        if payload.keys.is_empty() {
            return self.backend.delete().map_err(storage_error);
        }

        let serialized =
            serde_json::to_string(&payload).map_err(|_| storage_error(StoreError::Corrupt))?;
        let payload = Sensitive::new(serialized);
        self.backend.write(&payload).map_err(storage_error)
    }

    /// Replace the complete normalized key list in one native-store write.
    /// Existing IDs are retained when the same secret is supplied again.
    pub fn set_keys(&self, input: &str) -> Result<Vec<KeyMetadata>, AppError> {
        let normalized = normalize_api_keys(input)?;
        let _guard = self
            .operation
            .lock()
            .map_err(|_| storage_error(StoreError::Failed))?;
        let current = self.load_payload()?;

        let mut next = Vec::with_capacity(normalized.len());
        for secret in normalized {
            let id = current
                .keys
                .iter()
                .find(|existing| existing.secret == secret)
                .map(|existing| existing.id.clone())
                .unwrap_or_default();
            next.push(StoredKey { id, secret });
        }

        self.save_payload(StoredPayload {
            version: PAYLOAD_VERSION,
            keys: next.clone(),
        })?;
        Ok(next.iter().map(stored_metadata).collect())
    }

    /// Alias matching the IPC command's set terminology.
    pub fn set(&self, input: &str) -> Result<Vec<KeyMetadata>, AppError> {
        self.set_keys(input)
    }

    pub fn list_keys(&self) -> Result<Vec<KeyMetadata>, AppError> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| storage_error(StoreError::Failed))?;
        Ok(self
            .load_payload()?
            .keys
            .iter()
            .map(stored_metadata)
            .collect())
    }

    pub fn list(&self) -> Result<Vec<KeyMetadata>, AppError> {
        self.list_keys()
    }

    pub fn delete_key(&self, id: &KeyId) -> Result<Vec<KeyMetadata>, AppError> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| storage_error(StoreError::Failed))?;
        let mut payload = self.load_payload()?;
        payload.keys.retain(|key| &key.id != id);
        self.save_payload(payload.clone())?;
        Ok(payload.keys.iter().map(stored_metadata).collect())
    }

    pub fn delete(&self, id: &KeyId) -> Result<Vec<KeyMetadata>, AppError> {
        self.delete_key(id)
    }

    /// Load key material for the Gemini KeyProvider port. This method is
    /// intentionally not exposed through IPC.
    pub(crate) fn materials(&self) -> Result<Vec<KeyMaterial>, AppError> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| storage_error(StoreError::Failed))?;
        Ok(self
            .load_payload()?
            .keys
            .into_iter()
            .map(|key| KeyMaterial {
                id: key.id,
                secret: key.secret,
            })
            .collect())
    }
}

fn stored_metadata(key: &StoredKey) -> KeyMetadata {
    KeyMetadata {
        id: key.id.clone(),
        label: masked_label(key.secret.expose()),
    }
}

fn has_duplicate_ids_or_secrets(keys: &[StoredKey]) -> bool {
    keys.iter().enumerate().any(|(index, key)| {
        keys[..index]
            .iter()
            .any(|previous| previous.id == key.id || previous.secret == key.secret)
    })
}

/// In-memory fake used by unit tests and by higher layers' deterministic
/// tests. It has the same wrapped-payload boundary as the native adapter and
/// supports one-shot failures without ever logging the payload.
#[derive(Clone, Default)]
pub struct FakeCredentialStore {
    state: Arc<Mutex<FakeStoreState>>,
}

#[derive(Default)]
struct FakeStoreState {
    payload: Option<Sensitive<String>>,
    fail_read: bool,
    fail_write: bool,
    fail_delete: bool,
}

impl FakeCredentialStore {
    pub fn fail_next_read(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.fail_read = true;
        }
    }

    pub fn fail_next_write(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.fail_write = true;
        }
    }

    pub fn fail_next_delete(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.fail_delete = true;
        }
    }

    pub fn stored_payload(&self) -> Option<Sensitive<String>> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.payload.clone())
    }
}

impl CredentialStore for FakeCredentialStore {
    fn read(&self) -> Result<Option<Sensitive<String>>, StoreError> {
        let mut state = self.state.lock().map_err(|_| StoreError::Failed)?;
        if state.fail_read {
            state.fail_read = false;
            return Err(StoreError::Failed);
        }
        Ok(state.payload.clone())
    }

    fn write(&self, payload: &Sensitive<String>) -> Result<(), StoreError> {
        let mut state = self.state.lock().map_err(|_| StoreError::Failed)?;
        if state.fail_write {
            state.fail_write = false;
            return Err(StoreError::Failed);
        }
        state.payload = Some(payload.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), StoreError> {
        let mut state = self.state.lock().map_err(|_| StoreError::Failed)?;
        if state.fail_delete {
            state.fail_delete = false;
            return Err(StoreError::Failed);
        }
        state.payload = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(suffix: &str) -> String {
        format!("AIzaSyTESTKEY{suffix}1234")
    }

    #[test]
    fn normalize_trims_ignores_empty_and_deduplicates_without_exposing_values() {
        let values = normalize_api_keys(&format!("  {}, , {}, AQ.token  ", key("A"), key("A")))
            .expect("valid keys should normalize");
        assert_eq!(values.len(), 2);
        assert_eq!(masked_label(values[0].expose()), "AIza••••1234");
        assert!(!format!("{:?}", values[0]).contains("TESTKEY"));
    }

    #[test]
    fn masked_label_never_reveals_all_of_a_short_accepted_key() {
        assert_eq!(masked_label("AIzaX"), "AIza••••");
        assert_eq!(masked_label("AQ.X"), "AQ.••••");
    }

    #[test]
    fn unsupported_key_rejects_the_whole_input_before_store_write() {
        let store = FakeCredentialStore::default();
        let service = SecretService::new(store.clone());
        let error = service
            .set_keys(&format!("{},not-a-key", key("A")))
            .expect_err("invalid input must fail");
        assert_eq!(error.category, crate::core::error::Category::Format);
        assert!(store.stored_payload().is_none());
    }

    #[test]
    fn fake_store_round_trips_metadata_and_deletes_the_real_credential() {
        let store = FakeCredentialStore::default();
        let service = SecretService::new(store.clone());
        let metadata = service
            .set_keys(&format!(" {},{} ", key("A"), "AQ.tokenB"))
            .unwrap();
        assert_eq!(metadata.len(), 2);
        assert!(metadata.iter().all(|item| !item.label.contains("TESTKEY")));
        assert!(service
            .list_keys()
            .unwrap()
            .iter()
            .all(|item| item.label.contains("••••")));

        let remaining = service.delete_key(&metadata[0].id).unwrap();
        assert_eq!(remaining.len(), 1);
        let payload = store.stored_payload().unwrap();
        assert!(!format!("{:?}", payload).contains("TESTKEY"));

        service.delete_key(&remaining[0].id).unwrap();
        assert!(store.stored_payload().is_none());
    }

    #[test]
    fn store_failure_maps_to_storage_without_losing_existing_value() {
        let store = FakeCredentialStore::default();
        let service = SecretService::new(store.clone());
        service.set_keys(&key("A")).unwrap();
        store.fail_next_write();
        let error = service.set_keys(&key("B")).unwrap_err();
        assert_eq!(error.category, crate::core::error::Category::Storage);
        let listed = service.list_keys().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "AIza••••1234");
    }

    #[test]
    fn corrupted_payload_is_storage_error_and_never_returns_secret() {
        let store = FakeCredentialStore::default();
        let payload = Sensitive::new(
            r#"{"version":1,"keys":[{"id":"bad","secret":"AIzaSyCORRUPT"}]}"#.to_string(),
        );
        store.write(&payload).unwrap();
        let service = SecretService::new(store);
        let error = service.list_keys().unwrap_err();
        assert_eq!(error.category, crate::core::error::Category::Storage);
        assert!(!error.detail_redacted.contains("CORRUPT"));
    }

    #[test]
    fn key_material_never_appears_in_settings_database_or_metadata_response() {
        let raw_key = key("NOLEAK");
        let store = FakeCredentialStore::default();
        let service = SecretService::new(store);
        let metadata = service.set_keys(&raw_key).unwrap();

        let settings = crate::settings::Settings::default();
        let serialized_settings = serde_json::to_vec(&settings).unwrap();
        let serialized_metadata = serde_json::to_vec(&metadata).unwrap();
        assert!(!serialized_settings
            .windows(raw_key.len())
            .any(|window| window == raw_key.as_bytes()));
        assert!(!serialized_metadata
            .windows(raw_key.len())
            .any(|window| window == raw_key.as_bytes()));

        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::Db::open(dir.path()).unwrap();
        crate::settings::save(&db, &settings).unwrap();
        drop(db);
        for entry in std::fs::read_dir(dir.path()).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let bytes = std::fs::read(path).unwrap();
                assert!(
                    !bytes
                        .windows(raw_key.len())
                        .any(|window| window == raw_key.as_bytes()),
                    "settings persistence must not contain API key bytes"
                );
            }
        }
    }
}
