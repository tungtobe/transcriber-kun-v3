---
title: 'Story 6.5: Signed remote publishing and weekly model smoke'
type: 'feature'
created: '2026-09-30'
status: 'in-review'
baseline_commit: '1f48859275779fe1e00f8e05a4fec11958ebe2e7'
route: 'full'
route_source: 'auto'
review: 'none'
review_source: 'pinned'
lenses_ran: []
review_loop_iteration: 0
context: ['_bmad-output/implementation-artifacts/epic-6-context.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The app can verify remote content but there is no reproducible way to sign/publish it or detect when Google preview models stop working. A format mismatch, leaked private key, or silent model break would leave ads/config unavailable.

**Approach:** Add a signing/validation tool that produces the exact Story 6.1 envelope and signed image digests, a GitHub Pages publication procedure, and a weekly/manual CI smoke probe for Live setup plus a default-model sample transcription. Keep signing keys and Gemini test key outside the repo.

## Boundaries & Constraints

**Always:** Sign `trans-kun.remote.v1\0` plus canonical `serde_json::to_vec(payload)` with Ed25519, envelope version 1; hash image bytes into signed manifest. Validate payload and images before publication, including 100KB/image and ad Creative format. Accept private key only from a path or secret outside tracked repo files. Document custody, recovery and key rotation, requiring a new client binary with its embedded public key. Weekly smoke uses a separate test API key, verifies Live reaches setupComplete and transcribe returns a valid sample Chunk, and fails visibly with model-update instructions.

**Never:** Commit private/API keys, silently deploy unsigned content, publish before final GitHub Pages origin and Privacy Policy URL plus key custodian are supplied, or let malformed ad responses change Live/Job/transcript/recording state.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Valid payload/image | JSON plus image, external signing key | Versioned signed envelope and digest, client verifies | Deterministic bytes/signature |
| Tampering/invalid source | Modified payload/image, bad schema, >100KB image | No publish; client rejects tampering and falls back | Clear typed failure, no secret output |
| Weekly models healthy | Test key, current Live/default transcribe models, sample fixture | Live setupComplete and one valid transcribe Chunk | CI succeeds |
| Weekly model failure | Invalid key/model/network or malformed response | CI fails and points owner to recommended-settings update guide | No key in logs |
| Publication | GitHub Pages target and final values supplied | Signed files plus Privacy Policy served over HTTPS; app compiled to that origin/key | Verify URLs before release |

</frozen-after-approval>

## Code Map

- `src-tauri/src/remote/mod.rs` -- canonical envelope domain, signature verification, image digest rules and production build constants.
- `src-tauri/src/ads/mod.rs`, `docs/recommended-settings-format.md` -- manifest and recommendation payload schemas.
- `src-tauri/src/gemini/live/mod.rs`, `src-tauri/src/transcribe/adapter.rs`, `src-tauri/src/core/model_defaults.rs` -- real model setup/transcription paths and defaults.
- `src-tauri/tests/fixtures/media/sample.wav` -- smoke fixture.
- `.github/workflows/test.yml` -- existing CI conventions; add separate scheduled/manual workflow.
- `_bmad-output/planning-artifacts/epics.md` -- Story 6.5 acceptance and Open Question 6.

## Tasks & Acceptance

**Execution:**
- [x] `tools/remote-publish/` or `src-tauri/src/bin/` -- sign/verify/build manifest tool sharing Rust JSON/Ed25519 contract -- reproducible publication.
- [x] Tests for signing tool and `remote/` -- verify client accepts generated envelopes and rejects modified payload/image -- prevent contract drift.
- [x] `.github/workflows/` and smoke target -- weekly/manual Live setup and transcribe fixture checks with test secret and actionable failure -- detect model drift.
- [x] `docs/` -- GitHub Pages upload/verification, Privacy Policy, key custody/rotation and recommendation update runbook -- safe operations.
- [ ] Build config/documentation -- wire final origin/public key after user provides URL/custodian -- production fetch.

**Acceptance Criteria:**
- Given signed ads/settings fixtures, when loaded through the remote verifier, then signatures and image digest pass; tampering fails.
- Given missing/invalid external signing key, when publishing, then no output is published and no secret is logged.
- Given weekly CI with a test key, when it runs, then it checks Live setup and default transcription and reports a failed model check with update instructions.
- Given a final GitHub Pages origin, Privacy Policy URL and key custodian, when release is prepared, then signed content and key configuration can be verified end to end.
- Given remote ad failure during concurrent work, when tested, then Live/Job/transcript/recording state remains unchanged.

## Implementation Notes

- GitHub Pages is selected; the user will provide its final origin later. The production origin, Privacy Policy URL, key custodian and smoke API secret are not available, so no publication or live network smoke has run.
- `remote-publish` builds and verifies the same Ed25519 envelope and image digests as the app; five publisher tests and the full Rust suite pass.
- The weekly/manual smoke workflow and binary compile. A missing-key dry run exits with a redacted error; live model verification remains pending the dedicated Actions secret.

## Spec Change Log

## Review Triage Log

## Design Notes

GitHub Pages is the chosen host. The final HTTPS origin/Privacy Policy URL and private-key custodian are pending from the user; publish/release remains open until these values arrive. The weekly workflow should support manual dispatch before the scheduled run.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml remote::` -- verifier/signing contract tests pass.
- `cargo check --manifest-path src-tauri/Cargo.toml` -- Rust targets compile.
- `npm test` -- frontend regression tests pass.
- `npm run check` -- Svelte bindings remain valid.
