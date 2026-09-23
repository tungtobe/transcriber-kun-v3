# ADR 2.2: Gemini inline transcribe capability gate

**Status:** Open — S2/OQ2 is not verified.
**Date:** 2026-09-23

## Context

Story 2.2 sends chunk audio inline and must not add a Files API upload, change
the selected model, or silently fall back to a different protocol. The
specialized `*-transcribe` Interactions path requires direct evidence that the
selected model accepts inline audio. Generic Interactions documentation shows
an inline `input.audio.data` shape, while the Transcribe examples use a Files
API URI. Those examples do not establish inline support for the specialized
model.

No Gemini API key was available in the implementation environment for a direct
request against the target model. Unit tests of the request shape are not
evidence that the service accepts that shape.

## Decision

Keep specialized inline Interactions capability disabled unless the model has
an explicit, verified inline profile. Do not infer that profile from an alias
or model name. When no verified profile exists, return the S2/OQ2 gate error
before sending a request. If a direct request later confirms that the API only
accepts Files API references, stop and obtain the product and architecture
decision required by OQ2; do not upload audio or switch models in this story.

The general `generateContent` path remains available only for models with a
verified general profile. Both paths serialize and size-check the complete
inline JSON request before it reaches the gateway.

## Evidence and follow-up

- The [Gemini Transcription guide](https://ai.google.dev/gemini-api/docs/transcribe)
  describes word annotations and the specialized model flow using Files API
  references; it does not show direct inline audio for that model.
- The [Interactions API guide](https://ai.google.dev/gemini-api/docs/generate-content/transcribe)
  documents the general inline audio data shape, which is insufficient to
  validate the specialized model capability.
- Revisit this gate when a direct, successful inline request can be made with
  the target specialized model. Record the model ID, request form, response
  status, and date without saving a key, audio, transcript, or response body.
