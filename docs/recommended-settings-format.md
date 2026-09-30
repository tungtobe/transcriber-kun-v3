# Recommended settings document

Story 6.2 reads `recommended-settings.json` through the signed remote gateway. The gateway accepts HTTPS responses only and verifies the envelope before the settings service parses the payload. Settings documents require a fresh network response; cache and embedded content are not used as a successful preview.

## Signed envelope

The outer JSON has this shape:

```json
{
  "version": 1,
  "payload": {
    "schemaVersion": 1,
    "settings": {},
    "templates": []
  },
  "signature": "BASE64_ED25519_SIGNATURE"
}
```

The signature is Ed25519 over `trans-kun.remote.v1\0` followed by the UTF-8 bytes of the canonical JSON encoding of `payload`. The canonical encoding is the Rust `serde_json::to_vec` representation used by `src-tauri/src/remote/mod.rs`; object keys are serialized in sorted order. The embedded public key verifies the signature. `version` is the envelope version; unsupported values are rejected.

## Payload

`schemaVersion` is required and must equal `1`. Unknown fields are ignored. Only the following setting properties are read:

```json
{
  "schemaVersion": 1,
  "settings": {
    "transcribeModel": "gemini-2.5-flash",
    "liveModel": "gemini-2.5-flash-live",
    "memoModel": "gemini-2.5-flash",
    "chunkMinutes": 5
  },
  "templates": [
    {
      "externalId": "meeting-minutes",
      "locale": "en",
      "name": "Meeting minutes",
      "prompt": "Summarize the meeting using its transcript: {transcript}"
    }
  ]
}
```

`settings` and `templates` may be omitted. Omitted settings remain unchanged. Model names must pass the local non-blank model validator and are stored without the optional `models/` prefix. `chunkMinutes` must be an integer from 1 through 60.

Templates may use only these IDs and locales:

- `meeting-minutes` or `bilingual-ja-vi`
- `vi`, `en`, or `ja`

Each `(locale, externalId)` pair may occur once. Names use the local template rule of 1–100 trimmed Unicode characters. Prompts use the local template rule of at most 20,000 Unicode characters and must contain `{transcript}`. A changed template applies only while its local copy still matches the built-in default or the last accepted recommendation. Local edits produce a conflict and are preserved.

Keys, consent, and every other field are ignored and never shown or written by this feature. Future `schemaVersion` values, invalid field types, invalid ranges, signature failures, and network failures return a redacted typed error and leave local data unchanged.
