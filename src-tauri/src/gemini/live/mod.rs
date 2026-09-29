//! Gemini Live's raw WebSocket transport.
//!
//! This module owns setup, reconnect and audio replay. The LiveSession actor
//! consumes [`LiveEvent`]s through the [`LiveGateway::run`] port.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, Notify};
use tokio_tungstenite::tungstenite::Message;

use crate::audio::{PcmChunk, OUTPUT_CHANNELS, OUTPUT_CHUNK_SAMPLES, OUTPUT_SAMPLE_RATE};
use crate::core::error::{AppError, Code};
use crate::core::sensitive::Sensitive;
use crate::gemini::keys::{KeyLease, KeyPoolHandle, Priority, ReportAction, RequestOutcome};
use crate::gemini::params::GEMINI_LIVE_WS_ENDPOINT;
use crate::gemini::{CancellationToken, ConsentSnapshot};
use crate::settings::{Settings, TranscribeLanguage};

const AUDIO_MIME: &str = "audio/pcm;rate=16000";
const MAX_UNCONFIRMED_CHUNKS: usize = 600;
const SLIDING_WINDOW_TARGET_TOKENS: &str = "16384";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const SETUP_COMPLETE_TIMEOUT: Duration = Duration::from_secs(10);
/// Ping cadence and the receive silence after which a half-open socket is
/// declared dead. Any inbound frame (including pong) resets the silence timer.
const PING_INTERVAL: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

pub type ConnectFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn LiveSocket>, LiveConnectError>> + Send + 'a>>;
pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), LiveTransportError>> + Send + 'a>>;
pub type ReceiveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<LiveReceiveMessage, LiveTransportError>> + Send + 'a>>;
pub type CloseFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// The close code is retained for protocol classification; the server's close
/// reason is intentionally discarded because it may contain sensitive data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveReceiveMessage {
    Text(String),
    Closed {
        code: Option<u16>,
    },
    /// A non-data frame (ping/pong): proves the peer is alive.
    Activity,
    Eof,
}

/// WebSocket connection seam. The API key stays wrapped while it crosses the
/// connector boundary and is never included in diagnostics.
pub trait LiveSocketConnector: Send + Sync + 'static {
    fn connect<'a>(&'a self, api_key: &'a Sensitive<String>) -> ConnectFuture<'a>;
}

/// One connected WebSocket. Implementations intentionally return only
/// content-free transport errors.
pub trait LiveSocket: Send {
    fn send_text<'a>(&'a mut self, text: String) -> SendFuture<'a>;
    fn receive_text<'a>(&'a mut self) -> ReceiveFuture<'a>;
    /// Sends a WebSocket ping used for liveness detection.
    fn ping<'a>(&'a mut self) -> SendFuture<'a>;
    fn close<'a>(&'a mut self) -> CloseFuture<'a>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveConnectError {
    HttpStatus(u16),
    Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveTransportError;

/// Events the Story 4.6 actor needs. Output audio and output transcription are
/// deliberately absent from this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveEvent {
    InputTranscription {
        text: Sensitive<String>,
        sample_start: u64,
        sample_end: u64,
    },
    AudioGap {
        start_sample: u64,
        end_sample: u64,
    },
    TurnComplete {
        sample_start: u64,
        sample_end: u64,
    },
    ConnectionChanged {
        state: LiveConnectionState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveFailure {
    Cancelled,
    SetupRejected,
    Gateway(AppError),
}

impl From<AppError> for LiveFailure {
    fn from(value: AppError) -> Self {
        Self::Gateway(value)
    }
}

#[derive(Debug, Clone)]
pub struct LiveRunConfig {
    pub model: String,
    pub language: TranscribeLanguage,
}

impl LiveRunConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            model: settings.live_model.clone(),
            language: settings.transcribe_language,
        }
    }
}

/// Clock and bounded jitter are injectable so reconnect timing is deterministic
/// in tests.
trait LiveClock: Send + Sync + 'static {
    fn jitter_percent(&self) -> i8;
    fn sleep<'a>(
        &'a self,
        duration: Duration,
        cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;
}

#[derive(Default)]
struct SystemLiveClock;

impl LiveClock for SystemLiveClock {
    fn jitter_percent(&self) -> i8 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        (nanos % 41) as i8 - 20
    }

    fn sleep<'a>(
        &'a self,
        duration: Duration,
        cancellation: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            tokio::select! {
                _ = tokio::time::sleep(duration) => false,
                _ = cancellation.cancelled() => true,
            }
        })
    }
}

#[derive(Clone)]
pub struct LiveGateway {
    connector: Arc<dyn LiveSocketConnector>,
    key_pool: KeyPoolHandle,
    clock: Arc<dyn LiveClock>,
    setup_timeout: Duration,
    idle_timeout: Duration,
    ping_interval: Duration,
}

impl LiveGateway {
    pub fn new(connector: Arc<dyn LiveSocketConnector>, key_pool: KeyPoolHandle) -> Self {
        Self {
            connector,
            key_pool,
            clock: Arc::new(SystemLiveClock),
            setup_timeout: SETUP_COMPLETE_TIMEOUT,
            idle_timeout: IDLE_TIMEOUT,
            ping_interval: PING_INTERVAL,
        }
    }

    pub fn production(key_pool: KeyPoolHandle) -> Self {
        Self::new(Arc::new(GeminiLiveSocketConnector), key_pool)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gemini::keys::{KeyProvider, SystemClock};
    use crate::secrets::{KeyId, KeyMaterial};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Notify;

    struct StaticProvider(Vec<KeyMaterial>);

    impl KeyProvider for StaticProvider {
        fn load_keys(&self) -> Result<Vec<KeyMaterial>, AppError> {
            Ok(self.0.clone())
        }
    }

    async fn test_pool(keys: Vec<KeyMaterial>) -> KeyPoolHandle {
        let (pool, actor) =
            KeyPoolHandle::channel(Arc::new(StaticProvider(keys)), Arc::new(SystemClock));
        tokio::spawn(actor.run());
        pool.refresh().await.unwrap();
        pool
    }

    fn key(id: &str, secret: &str) -> KeyMaterial {
        KeyMaterial::new(KeyId::from_opaque(id), secret.to_owned())
    }

    async fn receive_input_transcription(
        events: &mut mpsc::Receiver<LiveEvent>,
    ) -> Option<LiveEvent> {
        while let Some(event) = events.recv().await {
            if matches!(event, LiveEvent::InputTranscription { .. }) {
                return Some(event);
            }
        }
        None
    }

    fn chunk(start_sample: u64, sample: i16) -> PcmChunk {
        PcmChunk {
            start_sample,
            sample_rate: OUTPUT_SAMPLE_RATE,
            channels: OUTPUT_CHANNELS,
            samples: vec![sample; OUTPUT_CHUNK_SAMPLES],
            gated_samples: vec![sample; OUTPUT_CHUNK_SAMPLES],
            source_errors: Vec::new(),
        }
    }

    #[test]
    fn setup_json_matches_live_api_contract_for_each_source_language() {
        for (language, expected) in [
            (TranscribeLanguage::Auto, "ja"),
            (TranscribeLanguage::Ja, "ja"),
            (TranscribeLanguage::Vi, "vi"),
            (TranscribeLanguage::En, "en"),
        ] {
            let setup = build_setup_message(
                &LiveRunConfig {
                    model: "gemini-live-test".to_owned(),
                    language,
                },
                None,
            )
            .unwrap();
            let actual: serde_json::Value = serde_json::from_str(&setup).unwrap();
            let expected = serde_json::json!({
                "setup": {
                    "model": "models/gemini-live-test",
                    "generationConfig": {
                        "responseModalities": ["AUDIO"],
                        "translationConfig": { "targetLanguageCode": expected },
                    },
                    "sessionResumption": {},
                    "contextWindowCompression": {
                        "slidingWindow": { "targetTokens": SLIDING_WINDOW_TARGET_TOKENS },
                    },
                    "inputAudioTranscription": {},
                }
            });
            assert_eq!(actual, expected);
            assert!(actual["setup"].get("outputAudioTranscription").is_none());
            assert!(actual["setup"]["generationConfig"]
                .get("inputAudioTranscription")
                .is_none());
        }

        let resumed = build_setup_message(
            &LiveRunConfig {
                model: "models/gemini-live-test".to_owned(),
                language: TranscribeLanguage::Ja,
            },
            Some(&Sensitive::new("latest-resumption-handle".to_owned())),
        )
        .unwrap();
        let resumed: serde_json::Value = serde_json::from_str(&resumed).unwrap();
        assert_eq!(
            resumed["setup"]["sessionResumption"]["handle"],
            "latest-resumption-handle"
        );
    }

    #[test]
    fn audio_message_is_exact_little_endian_pcm_and_invalid_audio_is_rejected() {
        let message = build_audio_message(&chunk(0, 0x1234)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&message).unwrap();
        assert_eq!(value["realtimeInput"]["audio"]["mimeType"], AUDIO_MIME);
        assert!(value["realtimeInput"]["audio"].get("data").is_some());
        assert_eq!(value["realtimeInput"].as_object().unwrap().len(), 1);
        let encoded = value["realtimeInput"]["audio"]["data"].as_str().unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded.len(), OUTPUT_CHUNK_SAMPLES * 2);
        assert_eq!(&decoded[..2], &[0x34, 0x12]);

        let mut invalid = chunk(0, 1);
        invalid.gated_samples.pop();
        assert!(matches!(
            build_audio_message(&invalid),
            Err(LiveFailure::Gateway(AppError {
                code: Code::Format,
                ..
            }))
        ));
    }

    #[test]
    fn parser_keeps_only_setup_resumption_and_input_transcription_fields() {
        let parsed = parse_server_message(
            r#"{"setupComplete":{},"sessionResumptionUpdate":{"newHandle":"resume-1","resumable":true},"goAway":{"timeLeft":"1s"},"serverContent":{"inputTranscription":{"text":"spoken words"},"outputTranscription":{"text":"discard me"},"modelTurn":{"parts":[{"inlineData":{"data":"large-output-audio"}}]}}}"#,
        )
        .unwrap();
        assert!(parsed.setup_complete);
        assert!(parsed.go_away);
        assert_eq!(parsed.input_transcription.as_deref(), Some("spoken words"));
        assert!(!parsed.turn_complete);
        assert_eq!(parsed.resumption_handle, Some(Some("resume-1".to_owned())));
        assert_eq!(
            parse_server_message(r#"{"sessionResumptionUpdate":{"resumable":true}}"#)
                .unwrap()
                .resumption_handle,
            None,
            "an update without a replacement handle preserves the latest handle"
        );
        assert_eq!(
            parse_server_message(r#"{"sessionResumptionUpdate":{"resumable":false}}"#)
                .unwrap()
                .resumption_handle,
            Some(None),
            "a non-resumable update clears the saved handle"
        );
    }

    #[test]
    fn parser_recognizes_turn_complete_without_reading_output_payloads() {
        let parsed = parse_server_message(
            r#"{"serverContent":{"turnComplete":{},"inputTranscription":{"text":"done."},"outputTranscription":{"text":"ignored"}}}"#,
        )
        .unwrap();
        assert!(parsed.turn_complete);
        assert_eq!(parsed.input_transcription.as_deref(), Some("done."));
    }

    #[test]
    fn ring_keeps_the_newest_six_hundred_chunks_and_reports_dropped_samples() {
        let mut ring = AudioRing::default();
        for index in 0..=MAX_UNCONFIRMED_CHUNKS {
            ring.push(chunk(
                index as u64 * OUTPUT_CHUNK_SAMPLES as u64,
                index as i16,
            ));
        }
        assert_eq!(ring.chunks.len(), MAX_UNCONFIRMED_CHUNKS);
        assert_eq!(ring.chunks.front().unwrap().chunk.start_sample, 1_600);
        assert_eq!(ring.chunks.back().unwrap().chunk.start_sample, 960_000);
        assert_eq!(
            ring.gaps.pop_front(),
            Some(GapRange {
                start: 0,
                end: 1_600,
            })
        );
    }

    #[test]
    fn healthy_sixty_minute_sample_clock_does_not_create_disconnect_gaps() {
        let mut ring = AudioRing::default();
        for index in 0..36_000_u64 {
            ring.push(chunk(index * OUTPUT_CHUNK_SAMPLES as u64, index as i16));
            ring.mark_sent(index);
        }
        assert_eq!(ring.chunks.len(), MAX_UNCONFIRMED_CHUNKS);
        assert!(ring.gaps.is_empty());
        assert_eq!(
            ring.observed_end,
            Some(36_000 * OUTPUT_CHUNK_SAMPLES as u64)
        );
    }

    #[test]
    fn outage_retains_six_hundred_and_reports_exactly_twelve_hundred_unsent_chunks() {
        let mut ring = AudioRing::default();
        for index in 0..MAX_UNCONFIRMED_CHUNKS as u64 {
            ring.push(chunk(index * OUTPUT_CHUNK_SAMPLES as u64, index as i16));
            ring.mark_sent(index);
        }
        for index in MAX_UNCONFIRMED_CHUNKS as u64..2_400_u64 {
            ring.push(chunk(index * OUTPUT_CHUNK_SAMPLES as u64, index as i16));
        }

        assert_eq!(ring.chunks.len(), 600);
        assert_eq!(
            ring.chunks.front().unwrap().chunk.start_sample,
            1_800 * 1_600
        );
        assert_eq!(
            ring.chunks.back().unwrap().chunk.start_sample,
            2_399 * 1_600
        );
        assert_eq!(ring.gaps.len(), 1);
        assert_eq!(
            ring.gaps[0],
            GapRange {
                start: 600 * 1_600,
                end: 1_800 * 1_600,
            }
        );
    }

    #[test]
    fn sixty_minute_matrix_has_six_disconnects_and_only_unsent_outage_gap() {
        let outages = [
            (1_000_u64, 4_u64),
            (4_000, 1),
            (8_000, 1_800),
            (12_000, 3),
            (16_000, 1),
            (20_000, 2),
            (30_000, 1),
        ];
        let reconnects = outages
            .iter()
            .map(|(start, len)| start + len)
            .collect::<std::collections::HashSet<_>>();
        let mut ring = AudioRing::default();
        let mut disconnect_count = 0;
        for index in 0..36_000_u64 {
            if reconnects.contains(&index) {
                for item in &mut ring.chunks {
                    item.ever_sent = true;
                }
                disconnect_count += 1;
            }
            ring.push(chunk(index * 1_600, index as i16));
            if !outages
                .iter()
                .any(|(start, len)| (*start..start + len).contains(&index))
            {
                ring.mark_sent(index);
            }
        }

        assert!(disconnect_count >= 6);
        assert_eq!(ring.chunks.len(), 600);
        assert_eq!(ring.gaps.len(), 1);
        assert_eq!(ring.gaps[0].end - ring.gaps[0].start, 1_200 * 1_600);
        assert_eq!(ring.gaps[0].start, 8_000 * 1_600);
    }

    #[test]
    fn lagged_ring_records_the_missing_span_once_and_stays_contiguous() {
        let mut ring = AudioRing::default();
        ring.push(chunk(0, 1));
        ring.note_lagged(3);
        assert_eq!(
            ring.gaps.pop_front(),
            Some(GapRange {
                start: 1_600,
                end: 1_600 + 3 * 1_600,
            })
        );
        // The next received chunk follows the skipped span: no second gap.
        ring.push(chunk(4 * 1_600, 2));
        assert!(ring.gaps.is_empty());
        assert_eq!(ring.observed_end, Some(5 * 1_600));
        // A lag before any chunk was observed has no known start.
        let mut empty = AudioRing::default();
        empty.note_lagged(2);
        assert!(empty.gaps.is_empty());
    }

    #[tokio::test]
    async fn audio_ingest_records_a_gap_when_the_broadcast_receiver_lags() {
        let (sender, receiver) = broadcast::channel(2);
        let ring = Arc::new(Mutex::new(AudioRing::default()));
        let changed = Arc::new(Notify::new());
        // Fill beyond capacity before the ingest task runs so it lags.
        for index in 0..5_u64 {
            sender
                .send(chunk(index * OUTPUT_CHUNK_SAMPLES as u64, index as i16))
                .unwrap();
        }
        let cancel = CancellationToken::new();
        let handle = spawn_audio_ingest(
            receiver,
            ring.clone(),
            changed,
            CancellationToken::new(),
            cancel.clone(),
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if ring.lock().unwrap().chunks.len() >= 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        let _ = handle.await;
        let ring = ring.lock().unwrap();
        // Chunks 0..3 were overwritten before the first read; chunks 3 and 4
        // arrived. The first observed chunk has no predecessor, so the lag
        // itself cannot be located, but the two survivors stay contiguous.
        assert!(ring.gaps.is_empty() || ring.gaps[0].end > ring.gaps[0].start);
        assert_eq!(ring.observed_end, Some(5 * OUTPUT_CHUNK_SAMPLES as u64));
    }

    #[test]
    fn transcript_end_uses_the_replay_cursor_when_nothing_was_sent_on_the_socket() {
        // Fresh socket after a reconnect: replay starts at the oldest chunk.
        assert_eq!(
            transcript_sample_end(None, Some(8_000), Some(96_000), 4_000),
            8_000
        );
        // Once audio was sent, that is authoritative.
        assert_eq!(
            transcript_sample_end(Some(20_000), Some(8_000), Some(96_000), 4_000),
            20_000
        );
        // Never behind the transcript cursor.
        assert_eq!(transcript_sample_end(None, Some(1_000), None, 4_000), 4_000);
        assert_eq!(transcript_sample_end(None, None, None, 4_000), 4_000);
    }

    #[test]
    fn reconnect_backoff_has_bounded_jitter_and_a_thirty_second_cap() {
        assert_eq!(reconnect_delay(0, -20), Duration::from_millis(800));
        assert_eq!(reconnect_delay(0, 20), Duration::from_millis(1_200));
        assert_eq!(reconnect_delay(3, 0), Duration::from_secs(8));
        assert_eq!(reconnect_delay(5, 20), Duration::from_secs(30));
        assert_eq!(reconnect_delay(10, -20), Duration::from_secs(30));
    }

    struct FakeClock {
        jitter: i8,
        delays: Mutex<Vec<Duration>>,
        sleep_started: Notify,
        wait_for_cancel: AtomicBool,
        released: Notify,
    }

    impl FakeClock {
        fn new(jitter: i8, wait_for_cancel: bool) -> Arc<Self> {
            Arc::new(Self {
                jitter,
                delays: Mutex::new(Vec::new()),
                sleep_started: Notify::new(),
                wait_for_cancel: AtomicBool::new(wait_for_cancel),
                released: Notify::new(),
            })
        }

        async fn wait_until_sleep_starts(&self) {
            self.sleep_started.notified().await;
        }
    }

    impl LiveClock for FakeClock {
        fn jitter_percent(&self) -> i8 {
            self.jitter
        }

        fn sleep<'a>(
            &'a self,
            duration: Duration,
            cancellation: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
            Box::pin(async move {
                self.delays.lock().unwrap().push(duration);
                self.sleep_started.notify_one();
                if self.wait_for_cancel.load(Ordering::Relaxed) {
                    tokio::select! {
                        _ = cancellation.cancelled() => true,
                        _ = self.released.notified() => false,
                    }
                } else {
                    tokio::task::yield_now().await;
                    cancellation.is_cancelled()
                }
            })
        }
    }

    enum ConnectStep {
        Error(LiveConnectError),
        Socket(FakeSocket),
        Pending,
    }

    struct FakeConnector {
        steps: Mutex<VecDeque<ConnectStep>>,
        keys: Mutex<Vec<String>>,
        connect_started: Notify,
    }

    impl FakeConnector {
        fn new(steps: impl IntoIterator<Item = ConnectStep>) -> Arc<Self> {
            Arc::new(Self {
                steps: Mutex::new(steps.into_iter().collect()),
                keys: Mutex::new(Vec::new()),
                connect_started: Notify::new(),
            })
        }
    }

    impl LiveSocketConnector for FakeConnector {
        fn connect<'a>(&'a self, api_key: &'a Sensitive<String>) -> ConnectFuture<'a> {
            Box::pin(async move {
                self.keys.lock().unwrap().push(api_key.expose().clone());
                self.connect_started.notify_one();
                let step = self.steps.lock().unwrap().pop_front();
                match step {
                    Some(ConnectStep::Error(error)) => Err(error),
                    Some(ConnectStep::Socket(socket)) => {
                        Ok(Box::new(socket) as Box<dyn LiveSocket>)
                    }
                    Some(ConnectStep::Pending) => std::future::pending().await,
                    None => Err(LiveConnectError::Transport),
                }
            })
        }
    }

    struct FakeSocket {
        incoming: VecDeque<Result<LiveReceiveMessage, LiveTransportError>>,
        sent: Arc<Mutex<Vec<String>>>,
        received: Arc<std::sync::atomic::AtomicUsize>,
        pending_receive: bool,
        pending_send: bool,
    }

    impl FakeSocket {
        fn new(
            messages: impl IntoIterator<Item = &'static str>,
        ) -> (
            Self,
            Arc<Mutex<Vec<String>>>,
            Arc<std::sync::atomic::AtomicUsize>,
        ) {
            let sent = Arc::new(Mutex::new(Vec::new()));
            let received = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            (
                Self {
                    incoming: messages
                        .into_iter()
                        .map(|message| Ok(LiveReceiveMessage::Text(message.to_owned())))
                        .collect(),
                    sent: sent.clone(),
                    received: received.clone(),
                    pending_receive: false,
                    pending_send: false,
                },
                sent,
                received,
            )
        }

        fn pending_receive() -> Self {
            Self {
                incoming: VecDeque::new(),
                sent: Arc::new(Mutex::new(Vec::new())),
                received: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                pending_receive: true,
                pending_send: false,
            }
        }

        fn with_incoming(
            incoming: impl IntoIterator<Item = Result<LiveReceiveMessage, LiveTransportError>>,
        ) -> Self {
            Self {
                incoming: incoming.into_iter().collect(),
                sent: Arc::new(Mutex::new(Vec::new())),
                received: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                pending_receive: false,
                pending_send: false,
            }
        }

        fn pending_send() -> Self {
            Self {
                incoming: VecDeque::new(),
                sent: Arc::new(Mutex::new(Vec::new())),
                received: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                pending_receive: false,
                pending_send: true,
            }
        }
    }

    impl LiveSocket for FakeSocket {
        fn send_text<'a>(&'a mut self, text: String) -> SendFuture<'a> {
            Box::pin(async move {
                if self.pending_send {
                    std::future::pending().await
                } else {
                    self.sent.lock().unwrap().push(text);
                    Ok(())
                }
            })
        }

        fn receive_text<'a>(&'a mut self) -> ReceiveFuture<'a> {
            Box::pin(async move {
                if self.pending_receive && self.incoming.is_empty() {
                    std::future::pending().await
                } else {
                    self.received.fetch_add(1, Ordering::Relaxed);
                    self.incoming
                        .pop_front()
                        .unwrap_or(Ok(LiveReceiveMessage::Eof))
                }
            })
        }

        fn ping<'a>(&'a mut self) -> SendFuture<'a> {
            Box::pin(async move {
                if self.pending_send {
                    std::future::pending().await
                } else {
                    Ok(())
                }
            })
        }

        fn close<'a>(&'a mut self) -> CloseFuture<'a> {
            Box::pin(async {})
        }
    }

    fn run_config() -> LiveRunConfig {
        LiveRunConfig {
            model: "gemini-live-test".to_owned(),
            language: TranscribeLanguage::En,
        }
    }

    #[tokio::test]
    async fn buffered_audio_does_not_starve_socket_close_or_connection_transition() {
        let socket = FakeSocket::with_incoming([
            Ok(LiveReceiveMessage::Text(
                r#"{"setupComplete":{}}"#.to_owned(),
            )),
            Ok(LiveReceiveMessage::Closed { code: Some(1000) }),
        ]);
        let sent = socket.sent.clone();
        let connector = FakeConnector::new([ConnectStep::Socket(socket), ConnectStep::Pending]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let clock = FakeClock::new(0, true);
        let gateway = LiveGateway::with_clock(connector, pool, clock);
        let (audio_tx, audio_rx) = broadcast::channel(1_201);
        for index in 0..1_200_u64 {
            audio_tx
                .send(chunk(index * OUTPUT_CHUNK_SAMPLES as u64, index as i16))
                .unwrap();
        }
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });

        let reconnect = tokio::time::timeout(Duration::from_secs(2), async {
            while let Some(event) = event_rx.recv().await {
                if matches!(
                    event,
                    LiveEvent::ConnectionChanged {
                        state: LiveConnectionState::Reconnecting
                    }
                ) {
                    return event;
                }
            }
            panic!("gateway stopped before reconnecting");
        })
        .await
        .expect("a queued close is polled despite buffered audio");
        assert!(matches!(
            reconnect,
            LiveEvent::ConnectionChanged {
                state: LiveConnectionState::Reconnecting
            }
        ));
        assert_eq!(sent.lock().unwrap().len(), 2, "setup plus one audio chunk");
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
    }

    #[tokio::test]
    async fn setup_complete_and_transcription_emit_connected_before_delta() {
        let (mut socket, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{},"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        socket.pending_receive = true;
        let connector = FakeConnector::new([ConnectStep::Socket(socket)]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::new(connector, pool);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });

        assert!(matches!(
            event_rx.recv().await,
            Some(LiveEvent::ConnectionChanged {
                state: LiveConnectionState::Connecting
            })
        ));
        assert!(matches!(
            event_rx.recv().await,
            Some(LiveEvent::ConnectionChanged {
                state: LiveConnectionState::Connected
            })
        ));
        assert!(matches!(
            event_rx.recv().await,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
    }

    #[tokio::test]
    async fn quota_rotation_goaway_resumption_replay_transcript_and_output_drop() {
        let (first_socket, first_sent, _first_received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"sessionResumptionUpdate":{"newHandle":"resume-new","resumable":true}}"#,
            r#"{"goAway":{"timeLeft":"1s"}}"#,
        ]);
        let (second_socket, second_sent, second_received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"hello"},"outputTranscription":{"text":"ignored"},"modelTurn":{"parts":[{"inlineData":{"data":"output audio"}}]}}}"#,
        ]);
        let connector = FakeConnector::new([
            ConnectStep::Error(LiveConnectError::HttpStatus(429)),
            ConnectStep::Socket(first_socket),
            ConnectStep::Socket(second_socket),
        ]);
        let clock = FakeClock::new(0, false);
        let pool = test_pool(vec![key("one", "secret-one"), key("two", "secret-two")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock.clone());
        let (audio_tx, audio_rx) = broadcast::channel(8);
        audio_tx.send(chunk(0, 0x0102)).unwrap();
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let gateway = gateway.clone();
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        let event = match tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await
        {
            Ok(Some(event)) => event,
            _ => {
                cancellation.cancel();
                let outcome = running.await.unwrap();
                panic!(
                    "transcript event timeout; outcome={outcome:?}, first_sends={}, second_sends={}, second_receives={}, connections={}",
                    first_sent.lock().unwrap().len(),
                    second_sent.lock().unwrap().len(),
                    second_received.load(Ordering::Relaxed),
                    connector.keys.lock().unwrap().len(),
                );
            }
        };
        assert!(matches!(
            event,
            LiveEvent::InputTranscription { ref text, .. } if text.expose() == "hello"
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));

        let keys = connector.keys.lock().unwrap().clone();
        assert_eq!(&keys[..2], &["secret-one", "secret-two"]);
        let first: Vec<serde_json::Value> = first_sent
            .lock()
            .unwrap()
            .iter()
            .map(|message| serde_json::from_str(message).unwrap())
            .collect();
        let second: Vec<serde_json::Value> = second_sent
            .lock()
            .unwrap()
            .iter()
            .map(|message| serde_json::from_str(message).unwrap())
            .collect();
        assert!(first
            .iter()
            .any(|message| message.get("realtimeInput").is_some()));
        assert!(second
            .iter()
            .any(|message| message.get("realtimeInput").is_some()));
        assert_eq!(
            second[0]["setup"]["sessionResumption"]["handle"],
            "resume-new"
        );
        // The gateway's final Stopped notification is best-effort so a full
        // event queue cannot block cancellation. The actor also sets Stopped
        // authoritatively when it receives GatewayEnded.
        assert!(clock.delays.lock().unwrap().len() >= 1);
    }

    #[tokio::test]
    async fn only_five_setup_rejections_are_terminal() {
        let connector = FakeConnector::new(
            (0..5).map(|_| ConnectStep::Error(LiveConnectError::HttpStatus(400))),
        );
        let clock = FakeClock::new(0, false);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, _event_rx) = mpsc::channel(32);
        let result = gateway
            .run(
                run_config(),
                ConsentSnapshot::new(1, false),
                audio_rx,
                event_tx,
                CancellationToken::new(),
            )
            .await;
        assert_eq!(result, Err(LiveFailure::SetupRejected));
        assert_eq!(connector.keys.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn close_codes_1007_and_1008_count_as_setup_rejections_before_setup_complete() {
        let close_codes = [Some(1007), Some(1008), Some(1007), Some(1008), Some(1007)];
        let connector = FakeConnector::new(close_codes.into_iter().map(|code| {
            ConnectStep::Socket(FakeSocket::with_incoming([Ok(
                LiveReceiveMessage::Closed { code },
            )]))
        }));
        let clock = FakeClock::new(0, false);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, _event_rx) = mpsc::channel(32);

        let result = gateway
            .run(
                run_config(),
                ConsentSnapshot::new(1, false),
                audio_rx,
                event_tx,
                CancellationToken::new(),
            )
            .await;

        assert_eq!(result, Err(LiveFailure::SetupRejected));
        assert_eq!(connector.keys.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn non_rejection_closes_and_server_errors_reconnect_without_counting() {
        let make_close = |code| {
            ConnectStep::Socket(FakeSocket::with_incoming([Ok(
                LiveReceiveMessage::Closed { code: Some(code) },
            )]))
        };
        let (mut ready_socket, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready_socket.pending_receive = true;
        let connector = FakeConnector::new([
            make_close(1000),
            make_close(1006),
            ConnectStep::Socket(FakeSocket::new([r#"{"error":{"code":500}}"#]).0),
            ConnectStep::Socket(FakeSocket::new([r#"{"error":{"code":503}}"#]).0),
            ConnectStep::Error(LiveConnectError::HttpStatus(500)),
            ConnectStep::Socket(ready_socket),
        ]);
        let clock = FakeClock::new(0, false);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock.clone());
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(2);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });

        let event = tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await
        .unwrap();
        assert!(matches!(
            event,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
        assert_eq!(connector.keys.lock().unwrap().len(), 6);
        assert_eq!(clock.delays.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn in_band_server_error_reconnects_without_using_the_fifth_setup_rejection() {
        let (mut ready_socket, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready_socket.pending_receive = true;
        let connector = FakeConnector::new(
            (0..4)
                .map(|_| ConnectStep::Error(LiveConnectError::HttpStatus(400)))
                .chain(std::iter::once(ConnectStep::Socket(
                    FakeSocket::new([r#"{"error":{"code":500}}"#]).0,
                )))
                .chain(std::iter::once(ConnectStep::Socket(ready_socket))),
        );
        let clock = FakeClock::new(0, false);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock.clone());
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(2);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });

        let event = tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await
        .unwrap();
        assert!(matches!(
            event,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
        assert_eq!(connector.keys.lock().unwrap().len(), 6);
        assert_eq!(clock.delays.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn close_code_1008_after_setup_complete_does_not_count_as_rejection() {
        let established_close = FakeSocket::with_incoming([
            Ok(LiveReceiveMessage::Text(
                r#"{"setupComplete":{}}"#.to_owned(),
            )),
            Ok(LiveReceiveMessage::Closed { code: Some(1008) }),
        ]);
        let rejections = (0..4).map(|_| {
            ConnectStep::Socket(FakeSocket::with_incoming([Ok(
                LiveReceiveMessage::Closed { code: Some(1007) },
            )]))
        });
        let (mut ready_socket, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready_socket.pending_receive = true;
        let connector = FakeConnector::new(
            std::iter::once(ConnectStep::Socket(established_close))
                .chain(rejections)
                .chain(std::iter::once(ConnectStep::Socket(ready_socket))),
        );
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, FakeClock::new(0, false));
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(2);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });

        let event = tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await
        .unwrap();
        assert!(matches!(
            event,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
        assert_eq!(connector.keys.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn missing_setup_complete_times_out_and_reconnects() {
        let (mut ready_socket, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready_socket.pending_receive = true;
        let connector = FakeConnector::new([
            ConnectStep::Socket(FakeSocket::pending_receive()),
            ConnectStep::Socket(ready_socket),
        ]);
        let clock = FakeClock::new(0, false);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let mut gateway = LiveGateway::with_clock(connector.clone(), pool, clock.clone());
        gateway.setup_timeout = Duration::from_millis(10);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(2);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });

        let event = tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await
        .unwrap();
        assert!(matches!(
            event,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
        assert_eq!(connector.keys.lock().unwrap().len(), 2);
        assert_eq!(clock.delays.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn stop_cancels_setup_wait_and_reconnect_sleep() {
        let connector = FakeConnector::new([ConnectStep::Socket(FakeSocket::pending_receive())]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::new(connector, pool);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, _event_rx) = mpsc::channel(32);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        tokio::task::yield_now().await;
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));

        let connector = FakeConnector::new([ConnectStep::Error(LiveConnectError::HttpStatus(404))]);
        let clock = FakeClock::new(0, true);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector, pool, clock.clone());
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, _event_rx) = mpsc::channel(32);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(2), clock.wait_until_sleep_starts())
            .await
            .unwrap();
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));

        let connector = FakeConnector::new([ConnectStep::Pending]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::new(connector.clone(), pool);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, _event_rx) = mpsc::channel(32);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(2), connector.connect_started.notified())
            .await
            .unwrap();
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));

        let connector = FakeConnector::new([ConnectStep::Socket(FakeSocket::pending_send())]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::new(connector.clone(), pool);
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, _event_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(2), connector.connect_started.notified())
            .await
            .unwrap();
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
    }

    #[tokio::test]
    async fn auth_rejections_rotate_keys_and_transport_errors_do_not_count_as_setup_rejections() {
        for status in [401, 403] {
            let (socket, _sent, _received) = FakeSocket::new([
                r#"{"setupComplete":{}}"#,
                r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
            ]);
            let connector = FakeConnector::new([
                ConnectStep::Error(LiveConnectError::HttpStatus(status)),
                ConnectStep::Socket(socket),
            ]);
            let pool = test_pool(vec![key("one", "secret-one"), key("two", "secret-two")]).await;
            let gateway =
                LiveGateway::with_clock(connector.clone(), pool, FakeClock::new(0, false));
            let (_audio_tx, audio_rx) = broadcast::channel(1);
            let (event_tx, mut event_rx) = mpsc::channel(2);
            let cancellation = CancellationToken::new();
            let running = tokio::spawn({
                let cancellation = cancellation.clone();
                async move {
                    gateway
                        .run(
                            run_config(),
                            ConsentSnapshot::new(1, false),
                            audio_rx,
                            event_tx,
                            cancellation,
                        )
                        .await
                }
            });
            assert!(matches!(
                tokio::time::timeout(
                    Duration::from_secs(2),
                    receive_input_transcription(&mut event_rx),
                )
                .await
                .unwrap(),
                Some(LiveEvent::InputTranscription { .. })
            ));
            cancellation.cancel();
            assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
            assert_eq!(
                connector.keys.lock().unwrap().as_slice(),
                &["secret-one", "secret-two"]
            );
        }

        let (mut socket, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        socket.pending_receive = true;
        let connector = FakeConnector::new([
            ConnectStep::Error(LiveConnectError::Transport),
            ConnectStep::Error(LiveConnectError::Transport),
            ConnectStep::Error(LiveConnectError::Transport),
            ConnectStep::Error(LiveConnectError::Transport),
            ConnectStep::Socket(socket),
        ]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let clock = FakeClock::new(0, false);
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock.clone());
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(2);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        let received = tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await;
        if received.is_err() {
            cancellation.cancel();
            let outcome = running.await.unwrap();
            panic!(
                "transport reconnect event timeout; outcome={outcome:?}, connections={}, delays={}",
                connector.keys.lock().unwrap().len(),
                clock.delays.lock().unwrap().len(),
            );
        }
        assert!(matches!(
            received.unwrap(),
            Some(LiveEvent::InputTranscription { .. })
        ));
        cancellation.cancel();
        assert_eq!(running.await.unwrap(), Err(LiveFailure::Cancelled));
        assert_eq!(connector.keys.lock().unwrap().len(), 5);
        assert_eq!(clock.delays.lock().unwrap().len(), 4);
    }

    #[test]
    fn binary_frames_decode_as_text_and_invalid_utf8_is_a_transport_error() {
        assert_eq!(
            decode_frame(Message::Binary(br#"{"setupComplete":{}}"#.to_vec().into())),
            Ok(LiveReceiveMessage::Text(
                r#"{"setupComplete":{}}"#.to_owned()
            ))
        );
        assert_eq!(
            decode_frame(Message::Binary(vec![0xff, 0xfe, 0x00].into())),
            Err(LiveTransportError)
        );
        assert_eq!(
            decode_frame(Message::Pong(Vec::new().into())),
            Ok(LiveReceiveMessage::Activity)
        );
    }

    #[test]
    fn in_band_error_code_tolerates_string_and_oversized_numbers() {
        let status = |raw: &str| parse_server_message(raw).unwrap().error_status;
        assert_eq!(status(r#"{"error":{"code":503}}"#), Some(503));
        assert_eq!(status(r#"{"error":{"code":"503"}}"#), Some(503));
        assert_eq!(status(r#"{"error":{"code":99999999999}}"#), Some(0));
        assert_eq!(status(r#"{"error":{"code":"UNAVAILABLE"}}"#), Some(0));
        assert_eq!(status(r#"{"error":{}}"#), Some(0));
        assert_eq!(status(r#"{"setupComplete":{}}"#), None);
    }

    async fn run_until_transcript(
        gateway: LiveGateway,
    ) -> (Option<LiveEvent>, Result<(), LiveFailure>) {
        let (_audio_tx, audio_rx) = broadcast::channel(1);
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let cancellation = CancellationToken::new();
        let running = tokio::spawn({
            let cancellation = cancellation.clone();
            async move {
                gateway
                    .run(
                        run_config(),
                        ConsentSnapshot::new(1, false),
                        audio_rx,
                        event_tx,
                        cancellation,
                    )
                    .await
            }
        });
        let event = tokio::time::timeout(
            Duration::from_secs(2),
            receive_input_transcription(&mut event_rx),
        )
        .await
        .ok()
        .flatten();
        cancellation.cancel();
        (event, running.await.unwrap())
    }

    #[tokio::test]
    async fn silent_established_socket_times_out_and_reconnects() {
        let (mut silent, _sent, _received) = FakeSocket::new([r#"{"setupComplete":{}}"#]);
        silent.pending_receive = true;
        let (mut ready, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready.pending_receive = true;
        let connector =
            FakeConnector::new([ConnectStep::Socket(silent), ConnectStep::Socket(ready)]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let mut gateway =
            LiveGateway::with_clock(connector.clone(), pool, FakeClock::new(0, false));
        gateway.idle_timeout = Duration::from_millis(30);
        gateway.ping_interval = Duration::from_millis(10);

        let (event, outcome) = run_until_transcript(gateway).await;
        assert!(matches!(
            event,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        assert_eq!(outcome, Err(LiveFailure::Cancelled));
        assert_eq!(connector.keys.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn in_band_error_after_setup_reconnects() {
        let (mut failing, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"error":{"code":"500","message":"boom"}}"#,
        ]);
        failing.pending_receive = true;
        let (mut ready, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready.pending_receive = true;
        let connector =
            FakeConnector::new([ConnectStep::Socket(failing), ConnectStep::Socket(ready)]);
        let pool = test_pool(vec![key("one", "secret-one")]).await;
        let gateway = LiveGateway::with_clock(connector.clone(), pool, FakeClock::new(0, false));

        let (event, _) = run_until_transcript(gateway).await;
        assert!(matches!(
            event,
            Some(LiveEvent::InputTranscription { ref text, .. }) if text.expose() == "ready"
        ));
        assert_eq!(connector.keys.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn key_status_retry_waits_for_backoff_before_the_next_key() {
        let (mut ready, _sent, _received) = FakeSocket::new([
            r#"{"setupComplete":{}}"#,
            r#"{"serverContent":{"inputTranscription":{"text":"ready"}}}"#,
        ]);
        ready.pending_receive = true;
        let connector = FakeConnector::new([
            ConnectStep::Error(LiveConnectError::HttpStatus(401)),
            ConnectStep::Socket(ready),
        ]);
        let pool = test_pool(vec![key("one", "secret-one"), key("two", "secret-two")]).await;
        let clock = FakeClock::new(0, false);
        let gateway = LiveGateway::with_clock(connector.clone(), pool, clock.clone());

        let (event, _) = run_until_transcript(gateway).await;
        assert!(event.is_some());
        assert_eq!(clock.delays.lock().unwrap().len(), 1);
    }
}
impl LiveGateway {
    #[cfg(test)]
    fn with_clock(
        connector: Arc<dyn LiveSocketConnector>,
        key_pool: KeyPoolHandle,
        clock: Arc<dyn LiveClock>,
    ) -> Self {
        Self {
            connector,
            key_pool,
            clock,
            setup_timeout: SETUP_COMPLETE_TIMEOUT,
            idle_timeout: IDLE_TIMEOUT,
            ping_interval: PING_INTERVAL,
        }
    }

    /// Run until the caller cancels, setup is rejected five times without a
    /// successful setup, or the key pool returns a terminal error. Transport
    /// failures and `goAway` reconnect indefinitely with cancellable backoff.
    pub async fn run(
        &self,
        config: LiveRunConfig,
        consent: ConsentSnapshot,
        audio: broadcast::Receiver<PcmChunk>,
        events: mpsc::Sender<LiveEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), LiveFailure> {
        if !consent.is_current() {
            return Err(
                AppError::new(Code::Blocked, "Gemini Live requires current consent").into(),
            );
        }
        validate_config(&config)?;

        let ring = Arc::new(Mutex::new(AudioRing::default()));
        let ring_changed = Arc::new(Notify::new());
        let ingest_stop = CancellationToken::new();
        let ingest = spawn_audio_ingest(
            audio,
            ring.clone(),
            ring_changed.clone(),
            cancellation.clone(),
            ingest_stop.clone(),
        );

        let result = self
            .run_loop(&config, ring, ring_changed, &events, cancellation.clone())
            .await;
        ingest_stop.cancel();
        let _ = ingest.await;
        let _ = events.try_send(LiveEvent::ConnectionChanged {
            state: LiveConnectionState::Stopped,
        });
        result
    }

    async fn run_loop(
        &self,
        config: &LiveRunConfig,
        ring: Arc<Mutex<AudioRing>>,
        ring_changed: Arc<Notify>,
        events: &mpsc::Sender<LiveEvent>,
        cancellation: CancellationToken,
    ) -> Result<(), LiveFailure> {
        let mut last_handle: Option<Sensitive<String>> = None;
        let mut next_lease: Option<KeyLease> = None;
        let mut setup_rejections = 0_u8;
        let mut backoff_attempt = 0_u32;
        let mut transcript_sample_cursor = 0_u64;

        if !emit_connection(events, LiveConnectionState::Connecting, &cancellation).await {
            return Err(LiveFailure::Cancelled);
        }

        loop {
            if cancellation.is_cancelled() {
                return Err(LiveFailure::Cancelled);
            }

            let lease = match next_lease.take() {
                Some(lease) => lease,
                None => match self
                    .key_pool
                    .acquire_cancellable(Priority::Live, &cancellation)
                    .await
                {
                    Ok(lease) => lease,
                    Err(error) if cancellation.is_cancelled() => {
                        let _ = error;
                        return Err(LiveFailure::Cancelled);
                    }
                    Err(error) => return Err(error.into()),
                },
            };

            let connected = tokio::select! {
                result = tokio::time::timeout(CONNECT_TIMEOUT, self.connector.connect(&lease.secret)) => {
                    match result {
                        Ok(result) => result,
                        Err(_) => Err(LiveConnectError::Transport),
                    }
                },
                _ = cancellation.cancelled() => {
                    let _ = self.key_pool.cancel(lease.request_id.clone()).await;
                    return Err(LiveFailure::Cancelled);
                }
            };

            match connected {
                Err(LiveConnectError::HttpStatus(status)) if is_key_status(status) => {
                    let outcome = outcome_for_status(status).expect("key status has an outcome");
                    match self.key_pool.report(lease, outcome).await {
                        Ok(ReportAction::Retry(next)) => {
                            next_lease = Some(next);
                            if !emit_connection(
                                events,
                                LiveConnectionState::Reconnecting,
                                &cancellation,
                            )
                            .await
                            {
                                return Err(LiveFailure::Cancelled);
                            }
                            if self.wait_before_retry(backoff_attempt, &cancellation).await {
                                return Err(LiveFailure::Cancelled);
                            }
                            backoff_attempt = backoff_attempt.saturating_add(1);
                            continue;
                        }
                        Ok(ReportAction::Complete) => {
                            if !emit_connection(
                                events,
                                LiveConnectionState::Reconnecting,
                                &cancellation,
                            )
                            .await
                            {
                                return Err(LiveFailure::Cancelled);
                            }
                            if self.wait_before_retry(backoff_attempt, &cancellation).await {
                                return Err(LiveFailure::Cancelled);
                            }
                            backoff_attempt = backoff_attempt.saturating_add(1);
                            continue;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                Err(LiveConnectError::HttpStatus(400 | 404)) => {
                    let _ = self.key_pool.report(lease, RequestOutcome::Request).await;
                    setup_rejections += 1;
                    if setup_rejections >= 5 {
                        return Err(LiveFailure::SetupRejected);
                    }
                    if !emit_connection(events, LiveConnectionState::Reconnecting, &cancellation)
                        .await
                    {
                        return Err(LiveFailure::Cancelled);
                    }
                    if self.wait_before_retry(backoff_attempt, &cancellation).await {
                        return Err(LiveFailure::Cancelled);
                    }
                    backoff_attempt = backoff_attempt.saturating_add(1);
                    continue;
                }
                Err(_) => {
                    let _ = self.key_pool.cancel(lease.request_id.clone()).await;
                    if !emit_connection(events, LiveConnectionState::Reconnecting, &cancellation)
                        .await
                    {
                        return Err(LiveFailure::Cancelled);
                    }
                    if self.wait_before_retry(backoff_attempt, &cancellation).await {
                        return Err(LiveFailure::Cancelled);
                    }
                    backoff_attempt = backoff_attempt.saturating_add(1);
                    continue;
                }
                Ok(socket) => {
                    let setup = build_setup_message(config, last_handle.as_ref())?;
                    let end = self
                        .run_socket(
                            socket,
                            lease,
                            setup,
                            ring.clone(),
                            ring_changed.clone(),
                            events,
                            cancellation.clone(),
                            last_handle.take(),
                            &mut transcript_sample_cursor,
                        )
                        .await?;

                    match end {
                        SocketEnd::Cancelled => return Err(LiveFailure::Cancelled),
                        SocketEnd::KeyRetry(next) => {
                            next_lease = Some(next);
                            if self.wait_before_retry(backoff_attempt, &cancellation).await {
                                return Err(LiveFailure::Cancelled);
                            }
                            backoff_attempt = backoff_attempt.saturating_add(1);
                            continue;
                        }
                        SocketEnd::Rejected(handle) => {
                            last_handle = handle;
                            setup_rejections = setup_rejections.saturating_add(1);
                            if setup_rejections >= 5 {
                                return Err(LiveFailure::SetupRejected);
                            }
                        }
                        SocketEnd::Reconnect {
                            handle,
                            established,
                        } => {
                            last_handle = handle;
                            if established {
                                setup_rejections = 0;
                                backoff_attempt = 0;
                            }
                        }
                    }
                    if !emit_connection(events, LiveConnectionState::Reconnecting, &cancellation)
                        .await
                    {
                        return Err(LiveFailure::Cancelled);
                    }
                    if self.wait_before_retry(backoff_attempt, &cancellation).await {
                        return Err(LiveFailure::Cancelled);
                    }
                    backoff_attempt = backoff_attempt.saturating_add(1);
                }
            }
        }
    }

    async fn run_socket(
        &self,
        mut socket: Box<dyn LiveSocket>,
        lease: KeyLease,
        setup: String,
        ring: Arc<Mutex<AudioRing>>,
        ring_changed: Arc<Notify>,
        events: &mpsc::Sender<LiveEvent>,
        cancellation: CancellationToken,
        mut handle: Option<Sensitive<String>>,
        transcript_sample_cursor: &mut u64,
    ) -> Result<SocketEnd, LiveFailure> {
        let mut lease = Some(lease);
        if tokio::select! {
            result = socket.send_text(setup) => result.is_err(),
            _ = cancellation.cancelled() => true,
        } {
            if cancellation.is_cancelled() {
                if let Some(lease) = lease.take() {
                    let _ = self.key_pool.cancel(lease.request_id).await;
                }
                return Ok(SocketEnd::Cancelled);
            }
            if let Some(lease) = lease.take() {
                let _ = self.key_pool.cancel(lease.request_id).await;
            }
            return Ok(SocketEnd::Reconnect {
                handle,
                established: false,
            });
        }

        let mut established = false;
        let mut last_sent_sequence = None;
        let mut last_sent_sample_end = None;
        let setup_timeout = tokio::time::sleep(self.setup_timeout);
        tokio::pin!(setup_timeout);
        let mut last_rx = tokio::time::Instant::now();
        let mut ping_timer = tokio::time::interval_at(
            tokio::time::Instant::now() + self.ping_interval,
            self.ping_interval,
        );
        ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let (invalid, gap, next_audio) = {
                let mut ring = ring.lock().expect("audio ring poisoned");
                let invalid = std::mem::take(&mut ring.invalid_chunk);
                let gap = ring.gaps.pop_front();
                let next_audio = if established {
                    ring.next_after(last_sent_sequence)
                } else {
                    None
                };
                (invalid, gap, next_audio)
            };
            if invalid {
                if let Some(lease) = lease.take() {
                    let _ = self.key_pool.cancel(lease.request_id).await;
                }
                return Err(AppError::new(
                    Code::Format,
                    "Gemini Live rejected an invalid PCM chunk",
                )
                .into());
            }
            if let Some(gap) = gap {
                if !emit_event(
                    events,
                    LiveEvent::AudioGap {
                        start_sample: gap.start,
                        end_sample: gap.end,
                    },
                    &cancellation,
                )
                .await
                {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Cancelled);
                }
                continue;
            }
            let mut sent_audio = false;
            if let Some((sequence, chunk)) = next_audio {
                let message = build_audio_message(&chunk)?;
                let send_failed = tokio::select! {
                    result = tokio::time::timeout(SEND_TIMEOUT, socket.send_text(message)) => {
                        !matches!(result, Ok(Ok(())))
                    },
                    _ = cancellation.cancelled() => {
                        if let Some(lease) = lease.take() {
                            let _ = self.key_pool.cancel(lease.request_id).await;
                        }
                        return Ok(SocketEnd::Cancelled);
                    }
                };
                if send_failed {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Reconnect {
                        handle,
                        established,
                    });
                }
                // This cursor prevents duplicate sends inside one socket only.
                // The ring still retains the chunk because send is not a model ACK.
                ring.lock()
                    .expect("audio ring poisoned")
                    .mark_sent(sequence);
                last_sent_sequence = Some(sequence);
                last_sent_sample_end = Some(
                    chunk
                        .start_sample
                        .saturating_add(chunk.samples.len() as u64),
                );
                sent_audio = true;
            }

            let incoming = tokio::select! {
                biased;
                _ = cancellation.cancelled() => {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Cancelled);
                }
                _ = &mut setup_timeout, if !established => {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    let _ = socket.close().await;
                    return Ok(SocketEnd::Reconnect {
                        handle,
                        established: false,
                    });
                }
                _ = tokio::time::sleep_until(last_rx + self.idle_timeout) => {
                    // Half-open socket: nothing (not even a pong) arrived.
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    let _ = socket.close().await;
                    return Ok(SocketEnd::Reconnect {
                        handle,
                        established,
                    });
                }
                _ = ping_timer.tick() => {
                    if !matches!(
                        tokio::time::timeout(SEND_TIMEOUT, socket.ping()).await,
                        Ok(Ok(()))
                    ) {
                        if let Some(lease) = lease.take() {
                            let _ = self.key_pool.cancel(lease.request_id).await;
                        }
                        return Ok(SocketEnd::Reconnect {
                            handle,
                            established,
                        });
                    }
                    None
                }
                result = socket.receive_text() => Some(result),
                _ = ring_changed.notified(), if !sent_audio => continue,
                // When the ring is continuously full, poll the socket once
                // after each audio send instead of taking the `continue` path
                // forever. This keeps close/goAway/transcript events flowing
                // while preserving a nonblocking audio producer.
                _ = tokio::time::sleep(Duration::ZERO), if sent_audio => None,
            };
            let Some(incoming) = incoming else {
                continue;
            };
            last_rx = tokio::time::Instant::now();
            let message = match incoming {
                Ok(LiveReceiveMessage::Activity) => continue,
                Ok(LiveReceiveMessage::Text(message)) => message,
                Ok(LiveReceiveMessage::Closed { code }) => {
                    if !established && matches!(code, Some(1007 | 1008)) {
                        if let Some(active_lease) = lease.take() {
                            let _ = self
                                .key_pool
                                .report(active_lease, RequestOutcome::Request)
                                .await;
                        }
                        return Ok(SocketEnd::Rejected(handle));
                    }
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Reconnect {
                        handle,
                        established,
                    });
                }
                Ok(LiveReceiveMessage::Eof) | Err(_) => {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Reconnect {
                        handle,
                        established,
                    });
                }
            };

            let parsed = match parse_server_message(&message) {
                Some(parsed) => parsed,
                None => {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Reconnect {
                        handle,
                        established,
                    });
                }
            };
            if let Some(next) = parsed.resumption_handle {
                handle = next.map(Sensitive::new);
            }
            if parsed.setup_complete && !established {
                let active_lease = lease.take().expect("setup lease exists until complete");
                self.key_pool
                    .report(active_lease, RequestOutcome::Success)
                    .await?;
                established = true;
                if !emit_connection(events, LiveConnectionState::Connected, &cancellation).await {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Cancelled);
                }
            }
            if let Some(text) = parsed.input_transcription {
                let sample_end = {
                    let ring = ring.lock().expect("audio ring poisoned");
                    transcript_sample_end(
                        last_sent_sample_end,
                        ring.replay_cursor(),
                        ring.observed_end,
                        *transcript_sample_cursor,
                    )
                };
                if !emit_event(
                    events,
                    LiveEvent::InputTranscription {
                        text: Sensitive::new(text),
                        sample_start: *transcript_sample_cursor,
                        sample_end,
                    },
                    &cancellation,
                )
                .await
                {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Cancelled);
                }
            }
            if parsed.turn_complete {
                let sample_end = {
                    let ring = ring.lock().expect("audio ring poisoned");
                    transcript_sample_end(
                        last_sent_sample_end,
                        ring.replay_cursor(),
                        ring.observed_end,
                        *transcript_sample_cursor,
                    )
                };
                if !emit_event(
                    events,
                    LiveEvent::TurnComplete {
                        sample_start: *transcript_sample_cursor,
                        sample_end,
                    },
                    &cancellation,
                )
                .await
                {
                    if let Some(lease) = lease.take() {
                        let _ = self.key_pool.cancel(lease.request_id).await;
                    }
                    return Ok(SocketEnd::Cancelled);
                }
                *transcript_sample_cursor = sample_end;
            }
            if let Some(status) = parsed.error_status.filter(|_| !established) {
                if let Some(outcome) = outcome_for_status(status) {
                    let active_lease = lease.take().expect("setup lease exists until complete");
                    return match self.key_pool.report(active_lease, outcome).await {
                        Ok(ReportAction::Retry(next)) => Ok(SocketEnd::KeyRetry(next)),
                        Ok(ReportAction::Complete) => Ok(SocketEnd::Reconnect {
                            handle,
                            established: false,
                        }),
                        Err(error) => Err(error.into()),
                    };
                }
                if matches!(status, 400 | 404) {
                    if let Some(active_lease) = lease.take() {
                        let _ = self
                            .key_pool
                            .report(active_lease, RequestOutcome::Request)
                            .await;
                    }
                    let _ = socket.close().await;
                    return Ok(SocketEnd::Rejected(handle));
                }
                if let Some(active_lease) = lease.take() {
                    let _ = self.key_pool.cancel(active_lease.request_id).await;
                }
                let _ = socket.close().await;
                return Ok(SocketEnd::Reconnect {
                    handle,
                    established: false,
                });
            }
            if established && parsed.error_status.is_some() {
                // An in-band error after setup ends the session server-side.
                let _ = socket.close().await;
                return Ok(SocketEnd::Reconnect {
                    handle,
                    established: true,
                });
            }
            if parsed.go_away {
                if let Some(active_lease) = lease.take() {
                    let _ = self.key_pool.cancel(active_lease.request_id).await;
                }
                let _ = socket.close().await;
                return Ok(SocketEnd::Reconnect {
                    handle,
                    established,
                });
            }
        }
    }

    async fn wait_before_retry(&self, attempt: u32, cancellation: &CancellationToken) -> bool {
        let delay = reconnect_delay(attempt, self.clock.jitter_percent());
        self.clock.sleep(delay, cancellation.clone()).await
    }
}

enum SocketEnd {
    Cancelled,
    KeyRetry(KeyLease),
    Rejected(Option<Sensitive<String>>),
    Reconnect {
        handle: Option<Sensitive<String>>,
        established: bool,
    },
}

#[derive(Default)]
struct ParsedServerMessage {
    setup_complete: bool,
    turn_complete: bool,
    go_away: bool,
    error_status: Option<u16>,
    input_transcription: Option<String>,
    /// Outer option means an update was present; inner option clears the saved
    /// handle when the server says the session is no longer resumable.
    resumption_handle: Option<Option<String>>,
}

fn parse_server_message(raw: &str) -> Option<ParsedServerMessage> {
    let value: WireServerMessage = serde_json::from_str(raw).ok()?;
    let mut parsed = ParsedServerMessage::default();
    parsed.setup_complete = value.setup_complete.is_some();
    parsed.go_away = value.go_away.is_some();
    parsed.turn_complete = value
        .server_content
        .as_ref()
        .is_some_and(|content| content.turn_complete.is_some());
    if let Some(update) = value.session_resumption_update {
        let resumable = update.resumable.unwrap_or(true);
        parsed.resumption_handle = if resumable {
            update.new_handle.map(Some)
        } else {
            Some(None)
        };
    }
    parsed.input_transcription = value
        .server_content
        .and_then(|content| content.input_transcription)
        .and_then(|transcription| transcription.text);
    // Presence of `error` is what matters; an unparsable code maps to 0 so it
    // is still treated as a (non-key, non-request) error.
    parsed.error_status = value
        .error
        .map(|error| error.code.and_then(WireErrorCode::status).unwrap_or(0));
    Some(parsed)
}

/// Unknown response fields are skipped by serde, including output audio
/// payloads. The transport never stores them in an intermediate JSON value.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireServerMessage {
    #[serde(default)]
    setup_complete: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    go_away: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    session_resumption_update: Option<WireResumptionUpdate>,
    #[serde(default)]
    server_content: Option<WireServerContent>,
    #[serde(default)]
    error: Option<WireServerError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResumptionUpdate {
    #[serde(default)]
    new_handle: Option<String>,
    #[serde(default)]
    resumable: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireServerContent {
    #[serde(default)]
    input_transcription: Option<WireTranscription>,
    #[serde(default)]
    turn_complete: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
struct WireTranscription {
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct WireServerError {
    #[serde(default)]
    code: Option<WireErrorCode>,
}

/// The server has sent numeric and string codes; both (and out-of-range
/// numbers) must parse without failing the whole message.
#[derive(Deserialize)]
#[serde(untagged)]
enum WireErrorCode {
    Number(serde_json::Number),
    Text(String),
    Other(serde::de::IgnoredAny),
}

impl WireErrorCode {
    fn status(self) -> Option<u16> {
        match self {
            Self::Number(number) => number.as_u64().and_then(|value| u16::try_from(value).ok()),
            Self::Text(text) => text.trim().parse::<u16>().ok(),
            Self::Other(_) => None,
        }
    }
}

fn validate_config(config: &LiveRunConfig) -> Result<(), LiveFailure> {
    if config.model.trim().is_empty() {
        return Err(AppError::new(Code::Model, "Gemini Live model is empty").into());
    }
    Ok(())
}

#[derive(Serialize)]
struct SetupEnvelope {
    setup: LiveSetup,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveSetup {
    model: String,
    generation_config: GenerationConfig,
    session_resumption: SessionResumption,
    context_window_compression: ContextWindowCompression,
    input_audio_transcription: EmptyObject,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    response_modalities: [&'static str; 1],
    translation_config: TranslationConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranslationConfig {
    target_language_code: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResumption {
    #[serde(skip_serializing_if = "Option::is_none")]
    handle: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextWindowCompression {
    sliding_window: SlidingWindow,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SlidingWindow {
    target_tokens: &'static str,
}

#[derive(Serialize)]
struct EmptyObject {}

fn build_setup_message(
    config: &LiveRunConfig,
    handle: Option<&Sensitive<String>>,
) -> Result<String, LiveFailure> {
    let model = if config.model.starts_with("models/") {
        config.model.clone()
    } else {
        format!("models/{}", config.model)
    };
    let target_language_code = match config.language {
        TranscribeLanguage::Auto => "ja",
        TranscribeLanguage::Ja => "ja",
        TranscribeLanguage::Vi => "vi",
        TranscribeLanguage::En => "en",
    };
    let handle = handle.map(|value| value.expose().clone());
    serde_json::to_string(&SetupEnvelope {
        setup: LiveSetup {
            model,
            generation_config: GenerationConfig {
                response_modalities: ["AUDIO"],
                translation_config: TranslationConfig {
                    target_language_code,
                },
            },
            session_resumption: SessionResumption { handle },
            context_window_compression: ContextWindowCompression {
                sliding_window: SlidingWindow {
                    target_tokens: SLIDING_WINDOW_TARGET_TOKENS,
                },
            },
            input_audio_transcription: EmptyObject {},
        },
    })
    .map_err(|_| AppError::new(Code::Shape, "Gemini Live setup could not be encoded").into())
}

fn build_audio_message(chunk: &PcmChunk) -> Result<String, LiveFailure> {
    if chunk.sample_rate != OUTPUT_SAMPLE_RATE
        || chunk.channels != OUTPUT_CHANNELS
        || chunk.gated_samples.len() != OUTPUT_CHUNK_SAMPLES
    {
        return Err(AppError::new(Code::Format, "Invalid Gemini Live PCM chunk").into());
    }
    // Only the noise-gated mix goes to Gemini; the archive keeps the raw one.
    let mut pcm = Vec::with_capacity(chunk.gated_samples.len() * 2);
    for sample in &chunk.gated_samples {
        pcm.extend_from_slice(&sample.to_le_bytes());
    }
    let payload = serde_json::json!({
        "realtimeInput": {
            "audio": {
                "data": base64::engine::general_purpose::STANDARD.encode(pcm),
                "mimeType": AUDIO_MIME,
            }
        }
    });
    serde_json::to_string(&payload)
        .map_err(|_| AppError::new(Code::Shape, "Gemini Live audio could not be encoded").into())
}

fn is_key_status(status: u16) -> bool {
    matches!(status, 401 | 403 | 429)
}

fn outcome_for_status(status: u16) -> Option<RequestOutcome> {
    match status {
        401 | 403 => Some(RequestOutcome::Auth),
        429 => Some(RequestOutcome::Quota),
        _ => None,
    }
}

fn reconnect_delay(attempt: u32, jitter_percent: i8) -> Duration {
    let jitter_percent = jitter_percent.clamp(-20, 20) as i128;
    let exponent = 1_u128.checked_shl(attempt.min(120)).unwrap_or(u128::MAX);
    let base_millis = 1_000_u128.saturating_mul(exponent);
    let jittered = base_millis.saturating_mul((100_i128 + jitter_percent) as u128) / 100;
    Duration::from_millis(jittered.min(30_000) as u64)
}

async fn emit_event(
    events: &mpsc::Sender<LiveEvent>,
    event: LiveEvent,
    cancellation: &CancellationToken,
) -> bool {
    tokio::select! {
        result = events.send(event) => result.is_ok(),
        _ = cancellation.cancelled() => false,
    }
}

async fn emit_connection(
    events: &mpsc::Sender<LiveEvent>,
    state: LiveConnectionState,
    cancellation: &CancellationToken,
) -> bool {
    emit_event(events, LiveEvent::ConnectionChanged { state }, cancellation).await
}

/// End sample for a transcription event. Audio sent on this socket is the
/// best evidence; before anything was sent (a fresh socket after a reconnect)
/// the replay cursor -- the first retained chunk -- is used, not the newest
/// observed sample, so replayed audio is not attributed to its future.
fn transcript_sample_end(
    last_sent_sample_end: Option<u64>,
    replay_cursor: Option<u64>,
    observed_end: Option<u64>,
    transcript_cursor: u64,
) -> u64 {
    last_sent_sample_end
        .or(replay_cursor)
        .or(observed_end)
        .unwrap_or(transcript_cursor)
        .max(transcript_cursor)
}

fn spawn_audio_ingest(
    mut receiver: broadcast::Receiver<PcmChunk>,
    ring: Arc<Mutex<AudioRing>>,
    changed: Arc<Notify>,
    external_cancel: CancellationToken,
    local_cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let received = tokio::select! {
                _ = external_cancel.cancelled() => break,
                _ = local_cancel.cancelled() => break,
                result = receiver.recv() => result,
            };
            match received {
                Ok(chunk) => {
                    ring.lock().expect("audio ring poisoned").push(chunk);
                    changed.notify_one();
                }
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    // The skipped chunks never reached the ring: record the
                    // missing span as a gap right away.
                    ring.lock()
                        .expect("audio ring poisoned")
                        .note_lagged(missed);
                    changed.notify_one();
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

#[derive(Clone)]
struct BufferedChunk {
    sequence: u64,
    chunk: PcmChunk,
    ever_sent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GapRange {
    start: u64,
    end: u64,
}

#[derive(Default)]
struct AudioRing {
    chunks: VecDeque<BufferedChunk>,
    gaps: VecDeque<GapRange>,
    next_sequence: u64,
    observed_end: Option<u64>,
    invalid_chunk: bool,
}

impl AudioRing {
    fn push(&mut self, chunk: PcmChunk) {
        if chunk.sample_rate != OUTPUT_SAMPLE_RATE
            || chunk.channels != OUTPUT_CHANNELS
            || chunk.samples.len() != OUTPUT_CHUNK_SAMPLES
        {
            self.invalid_chunk = true;
            return;
        }
        let end = chunk
            .start_sample
            .saturating_add(chunk.samples.len() as u64);
        if let Some(previous_end) = self.observed_end {
            if chunk.start_sample > previous_end {
                self.push_gap(GapRange {
                    start: previous_end,
                    end: chunk.start_sample,
                });
            }
        }
        self.observed_end = Some(self.observed_end.map_or(end, |value| value.max(end)));

        if self.chunks.len() == MAX_UNCONFIRMED_CHUNKS {
            if let Some(dropped) = self.chunks.pop_front() {
                // A send attempt is not an ACK, so retain recent sent chunks
                // for replay. If an older sent chunk ages out of this bounded
                // window, do not call it a known disconnect gap; only chunks
                // that never reached any socket are known to be unreplayable.
                if !dropped.ever_sent {
                    self.push_gap(GapRange {
                        start: dropped.chunk.start_sample,
                        end: dropped
                            .chunk
                            .start_sample
                            .saturating_add(dropped.chunk.samples.len() as u64),
                    });
                }
            }
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.chunks.push_back(BufferedChunk {
            sequence,
            chunk,
            ever_sent: false,
        });
    }

    /// Records the span of `missed` chunks skipped by a lagging broadcast
    /// receiver as a gap directly after the last observed sample.
    fn note_lagged(&mut self, missed: u64) {
        let Some(end) = self.observed_end else {
            return;
        };
        let span = missed.saturating_mul(OUTPUT_CHUNK_SAMPLES as u64);
        let gap_end = end.saturating_add(span);
        self.push_gap(GapRange {
            start: end,
            end: gap_end,
        });
        self.observed_end = Some(gap_end);
    }

    /// Where a replay on a fresh socket starts: the oldest retained chunk.
    fn replay_cursor(&self) -> Option<u64> {
        self.chunks.front().map(|item| item.chunk.start_sample)
    }

    fn push_gap(&mut self, gap: GapRange) {
        if gap.start >= gap.end {
            return;
        }
        if let Some(last) = self.gaps.back_mut() {
            if gap.start <= last.end {
                last.end = last.end.max(gap.end);
                return;
            }
        }
        self.gaps.push_back(gap);
    }

    fn next_after(&self, sequence: Option<u64>) -> Option<(u64, PcmChunk)> {
        self.chunks
            .iter()
            .find(|item| sequence.is_none_or(|last| item.sequence > last))
            .map(|item| (item.sequence, item.chunk.clone()))
    }

    fn mark_sent(&mut self, sequence: u64) {
        if let Some(item) = self
            .chunks
            .iter_mut()
            .find(|item| item.sequence == sequence)
        {
            item.ever_sent = true;
        }
    }
}

/// Production `wss://` connector. API keys are sent only as the endpoint's
/// required query parameter; neither the URL nor the error string is retained.
pub struct GeminiLiveSocketConnector;

impl LiveSocketConnector for GeminiLiveSocketConnector {
    fn connect<'a>(&'a self, api_key: &'a Sensitive<String>) -> ConnectFuture<'a> {
        Box::pin(async move {
            let mut url = reqwest::Url::parse(GEMINI_LIVE_WS_ENDPOINT)
                .map_err(|_| LiveConnectError::Transport)?;
            url.query_pairs_mut().append_pair("key", api_key.expose());
            match tokio_tungstenite::connect_async(url.as_str()).await {
                Ok((socket, _)) => {
                    tracing::debug!("Gemini Live WebSocket connected");
                    Ok(Box::new(TungsteniteLiveSocket { socket }) as Box<dyn LiveSocket>)
                }
                Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                    tracing::warn!(
                        status = response.status().as_u16(),
                        "Gemini Live WebSocket handshake rejected"
                    );
                    Err(LiveConnectError::HttpStatus(response.status().as_u16()))
                }
                Err(err) => {
                    // Display của lỗi tungstenite không chứa URL/khoá.
                    tracing::warn!(error = %err, "Gemini Live WebSocket connect failed");
                    Err(LiveConnectError::Transport)
                }
            }
        })
    }
}

/// Maps one WebSocket frame to a receive result. Gemini can deliver its JSON
/// as binary frames, so those are decoded as UTF-8 text; invalid UTF-8 is a
/// transport error (the caller reconnects).
fn decode_frame(frame: Message) -> Result<LiveReceiveMessage, LiveTransportError> {
    match frame {
        Message::Text(text) => Ok(LiveReceiveMessage::Text(text.to_string())),
        Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
            .map(LiveReceiveMessage::Text)
            .map_err(|_| LiveTransportError),
        Message::Close(frame) => {
            tracing::debug!(
                code = frame.as_ref().map(|frame| u16::from(frame.code)),
                "Gemini Live WebSocket close frame"
            );
            Ok(LiveReceiveMessage::Closed {
                code: frame.map(|frame| u16::from(frame.code)),
            })
        }
        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => Ok(LiveReceiveMessage::Activity),
    }
}

struct TungsteniteLiveSocket {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl LiveSocket for TungsteniteLiveSocket {
    fn send_text<'a>(&'a mut self, text: String) -> SendFuture<'a> {
        Box::pin(async move {
            self.socket
                .send(Message::Text(text.into()))
                .await
                .map_err(|_| LiveTransportError)
        })
    }

    fn receive_text<'a>(&'a mut self) -> ReceiveFuture<'a> {
        Box::pin(async move {
            match self.socket.next().await {
                Some(Ok(frame)) => decode_frame(frame),
                None => Ok(LiveReceiveMessage::Eof),
                Some(Err(_)) => Err(LiveTransportError),
            }
        })
    }

    fn ping<'a>(&'a mut self) -> SendFuture<'a> {
        Box::pin(async move {
            self.socket
                .send(Message::Ping(Vec::new().into()))
                .await
                .map_err(|_| LiveTransportError)
        })
    }

    fn close<'a>(&'a mut self) -> CloseFuture<'a> {
        Box::pin(async move {
            let _ = self.socket.close(None).await;
        })
    }
}
