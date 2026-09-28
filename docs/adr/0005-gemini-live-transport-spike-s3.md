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
meaning and the S3 spike confirms it. The ring tracks whether a chunk was ever
sent to any socket only to distinguish local replay backlog from chunks that
never reached a socket; that bit is not an acknowledgement.

When the ring evicts a chunk that was never sent to any socket, the transport
emits its original sample interval as a `disconnected` gap. Older sent-but-
unconfirmed chunks may age out silently so a healthy 60-minute stream does not
manufacture gaps after its first minute. This means an outage longer than the
60-second local ring can only report the unsent portion that aged out; the
retained newest 600 chunks are replayed on reconnect. Sent-but-unconfirmed audio
may have been received, lost, or duplicated by the server. Until the S3 evidence
below exists, this code makes no claim that reconnect replay is loss-free or
duplicate-free.

## Evidence available

The local fake-transport tests cover exact setup JSON, language mapping,
little-endian 16 kHz PCM encoding, all-part input parsing with output audio
discarded, healthy 36,000-chunk/60-minute simulation without false gaps,
1,800-chunk outage eviction with an exact 1,200-chunk gap and 600 retained
chunks, reconnect backoff, key-pool rotation, resumption-handle reuse, replay
after `goAway`, setup-rejection counting, and cancellation while connecting,
sending, receiving, or sleeping.
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
