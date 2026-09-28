# ADR 0005: Gemini Live transport and S3 spike evidence

## Status

Implementation complete; production behavior remains **not verified**. The
60-minute S3 server spike has not been run in this change.

## Decision

Story 4.4 uses the documented Gemini Live WebSocket protocol with a bounded
600-chunk PCM ring, the latest server-provided resumption handle, and
cancellable reconnects. It replays retained audio after reconnect. A successful
WebSocket send is not treated as server acknowledgement, and no server event
is treated as an audio-chunk acknowledgement unless Google documents that
meaning and the S3 spike confirms it.

When the ring evicts unconfirmed chunks, the transport emits their sample
interval as an audio gap. Until the S3 evidence below exists, this code makes
no claim that reconnect replay is loss-free or duplicate-free.

## Evidence available

The local fake-transport tests cover exact setup JSON, language mapping,
little-endian 16 kHz PCM encoding, all-part input parsing with output audio
discarded, 600-chunk eviction and gap reporting, reconnect backoff, key-pool
rotation, resumption-handle reuse, replay after `goAway`, setup-rejection
counting, and cancellation while connecting, sending, receiving, or sleeping.
These tests establish client behavior against scripted messages only; they do
not establish server acknowledgement or deduplication semantics.

No real API key or Gemini model was used for these tests.

## Required S3 spike before claiming no-loss/no-duplicate behavior

Run for 60 minutes in a controlled environment with at least six forced
disconnects. Record the following, without recording keys, full WebSocket URLs,
or transcript content:

- For each disconnect, the sample interval buffered, replayed, and observed
  after reconnect; distinguish send attempts from server-confirmed receipt.
- Whether the server supplies any documented per-audio acknowledgement and
  whether reconnect replay is deduplicated. Record the exact documented event
  or response field that proves acknowledgement.
- Whether the latest resumption handle is accepted, including behavior after
  handle expiry and after reconnecting without a handle.
- `goAway` delivery and the time from `goAway` to the next `setupComplete`.
- Whether interruption events occur and whether they affect input-audio replay.
- Confirmation that output audio is discarded and a cost observation for the
  chosen Live model/session configuration.

Until this evidence is added, acknowledgement, replay deduplication, handle
expiry recovery, and end-to-end loss/duplication behavior remain open.
