---
title: 'Story 6.1: Signed remote gateway'
type: 'feature'
created: '2026-09-30'
status: 'done'
baseline_commit: '97743ed326ee159826c3b088687a1bdc6bf5ebeb'
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

**Problem:** Ads and recommended settings need one trustworthy source of remote content. Unverified bytes, unsafe URLs, oversized images, and network failures must not affect transcription or recording.

**Approach:** Add a Rust `remote/` gateway for build-configured HTTPS resources, versioned Ed25519 signed JSON envelopes, digest-bound images, verified caching, and typed provenance/errors. Keep it independent of settings and feature consumers.

## Boundaries & Constraints

**Always:** GET only with no cookies or user identifiers; configured HTTPS origin only, safe redirects, finite timeouts and byte/decode limits; embedded public key; cache at `cache/remote/<sha256(url)>` with 24h TTL. Verified stale cache may rescue ads, but recommended settings requires a fresh valid response to count as a successful download. Failures are redacted and never panic.

**Never:** Execute HTML/script, trust server strings as filesystem paths, write settings, or let a failed remote request change Job/Live/transcript/recording state.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Signed JSON | Valid envelope from configured HTTPS source | Verified payload, `fresh`, cached | No error |
| Bad or offline response | Tampered signature, malformed JSON, expired cache, network failure | Ads use verified cache then embedded fallback; settings download reports failure | Classified, redacted error |
| Image | Digest listed in signed manifest; allowed PNG/JPEG/WebP, ≤100 KB and bounded dimensions | Verified image, hash-based cache filename | Reject MIME/bytes mismatch, oversize, digest mismatch |
| Outbound link | Creative HTTPS URL; report HTTPS or mailto | Safe URL with encoded creative ID | Reject custom/file/javascript schemes |

</frozen-after-approval>

## Code Map

- `src-tauri/src/remote/mod.rs` -- existing stub, exported by `src-tauri/src/lib.rs`; implement gateway and inline tests here or focused child modules.
- `src-tauri/Cargo.toml` -- reqwest, sha2, serde_json, Tokio present; add Ed25519 verifier dependency.
- `src-tauri/src/core/error.rs` -- reuse classified error codes and redaction conventions.
- `src-tauri/src/ipc/boot.rs` -- app data path pattern; derive cache path from container, never from remote fields.
- `_bmad-output/planning-artifacts/epics.md` -- Story 6.1 acceptance details; no settings or UI work in this story.

## Tasks & Acceptance

**Execution:**
- [x] `src-tauri/src/remote/` -- implement configured HTTP GET, bounded stream, envelope verifier, source/failure types, cache and fallbacks -- shared trust boundary.
- [x] `src-tauri/Cargo.toml` -- add pinned Ed25519 dependency -- signature verification.
- [x] `src-tauri/src/remote/` tests -- fake transport/clock and valid/tampered/oversize/offline cases -- prove isolation and fallback.
- [x] `src-tauri/src/remote/` -- validate image digest and safe outbound links -- reject unsafe content before consumers use it.

**Acceptance Criteria:**
- Given a valid signed response, when requested, then only its verified payload is returned and cached with `fresh` provenance.
- Given a failing or tampered response, when requested for ads, then only previously verified cache or embedded content is returned with typed provenance/error.
- Given a failing recommended-settings request, when requested, then the caller receives failure rather than a success based on stale cache or fallback.
- Given an oversized or mismatched image, when fetched, then it is rejected before unsafe allocation or persistence.
- Given an unsafe click/report URL, when validated, then it cannot be opened.

## Implementation Notes

- Signed payload bytes are `trans-kun.remote.v1\0` followed by `serde_json::to_vec(payload)`; Story 6.5 must produce the identical bytes.
- Gateway origin and public key are compile-time configuration. Missing values are a typed error until publishing configuration is set.
- Remote tests passed 9/9; `cargo check` and `git diff --check` passed.

## Spec Change Log

## Review Triage Log

## Design Notes

Use one canonical signed-byte contract that Story 6.5's signing tool can reproduce. Keep production URL/public key build-configured so Story 6.5 can supply final publication values after Open Question 6 is settled.

## Verification

**Commands:**
- `cargo test --manifest-path src-tauri/Cargo.toml remote::` -- remote boundary cases pass.
- `cargo check --manifest-path src-tauri/Cargo.toml` -- crate compiles.
