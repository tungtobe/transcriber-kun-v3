use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use trans_kun_lib::consent::CURRENT_VERSION;
use trans_kun_lib::core::model_defaults;
use trans_kun_lib::gemini::keys::{KeyPoolHandle, KeyProvider, SystemClock};
use trans_kun_lib::gemini::live::LiveConnectionState;
use trans_kun_lib::gemini::{
    CancellationToken, ConsentSnapshot, GeminiGateway, LiveEvent, LiveGateway, LiveRunConfig,
    ReqwestTransport,
};
use trans_kun_lib::media::{self, Chunk, ChunkOptions, Chunker};
use trans_kun_lib::secrets::{normalize_api_keys, KeyId, KeyMaterial};
use trans_kun_lib::settings::{LiveTarget, TranscribeLanguage};
use trans_kun_lib::transcribe::adapter::transcribe_chunk;

struct SmokeKeyProvider {
    key: String,
}

impl KeyProvider for SmokeKeyProvider {
    fn load_keys(&self) -> Result<Vec<KeyMaterial>, trans_kun_lib::core::error::AppError> {
        Ok(vec![KeyMaterial::new(KeyId::new(), self.key.clone())])
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let Some(raw_key) = std::env::var_os("GEMINI_SMOKE_API_KEY") else {
        return fail("GEMINI_SMOKE_API_KEY is missing");
    };
    let raw_key = raw_key.to_string_lossy();
    let Ok(mut normalized) = normalize_api_keys(raw_key.as_ref()) else {
        return fail("GEMINI_SMOKE_API_KEY is invalid");
    };
    if normalized.len() != 1 {
        return fail("GEMINI_SMOKE_API_KEY must contain exactly one key");
    }
    let key = normalized.remove(0).into_inner();

    let provider = Arc::new(SmokeKeyProvider { key });
    let (key_pool, actor) = KeyPoolHandle::channel(provider, Arc::new(SystemClock));
    let actor_task = tokio::spawn(actor.run());
    if key_pool.refresh().await.is_err() {
        return fail("could not initialize the smoke key pool");
    }

    let consent = ConsentSnapshot::new(CURRENT_VERSION, false);
    let live_gateway = LiveGateway::production(key_pool.clone());
    let live_ok = live_setup_smoke(live_gateway, consent).await;

    let transcribe_ok = match transcribe_smoke(key_pool, consent).await {
        Ok(()) => true,
        Err(()) => false,
    };
    actor_task.abort();

    if live_ok {
        println!("Live setup smoke: passed");
    } else {
        eprintln!("::error::Live setup smoke failed. Check GEMINI_SMOKE_API_KEY and the default Live model. If the model was retired or changed, update src-tauri/src/core/model_defaults.rs and docs/recommended-settings-format.md.");
    }
    if transcribe_ok {
        println!("Default transcription smoke: passed");
    } else {
        eprintln!("::error::Default transcription smoke failed. Check GEMINI_SMOKE_API_KEY and the default transcription model. If the model was retired or changed, update src-tauri/src/core/model_defaults.rs and docs/recommended-settings-format.md.");
    }

    if live_ok && transcribe_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

async fn live_setup_smoke(gateway: LiveGateway, consent: ConsentSnapshot) -> bool {
    let config = LiveRunConfig {
        model: model_defaults::DEFAULT_LIVE_MODEL.to_owned(),
        language: TranscribeLanguage::Auto,
        target: LiveTarget::None,
        start_sample: Arc::new(AtomicU64::new(0)),
    };
    let (_audio_source, audio) = broadcast::channel(2);
    let (events_tx, mut events_rx) = mpsc::channel(8);
    let cancellation = CancellationToken::new();
    let cancel_for_task = cancellation.clone();
    let mut task = tokio::spawn(async move {
        gateway
            .run(config, consent, audio, events_tx, cancel_for_task)
            .await
    });

    let connected = timeout(Duration::from_secs(70), async {
        loop {
            match events_rx.recv().await {
                Some(LiveEvent::ConnectionChanged {
                    state: LiveConnectionState::Connected,
                }) => return true,
                Some(_) => {}
                None => return false,
            }
        }
    })
    .await
    .unwrap_or(false);
    cancellation.cancel();
    let stopped = timeout(Duration::from_secs(10), &mut task).await;
    if stopped.is_err() {
        task.abort();
        return false;
    }
    connected
        && matches!(
            stopped,
            Ok(Ok(Err(trans_kun_lib::gemini::LiveFailure::Cancelled)))
        )
}

async fn transcribe_smoke(key_pool: KeyPoolHandle, consent: ConsentSnapshot) -> Result<(), ()> {
    let gateway = GeminiGateway::new(Arc::new(ReqwestTransport::new().map_err(|_| ())?), key_pool);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/media/sample.wav");
    let mut chunker = Chunker::new(ChunkOptions::default()).map_err(|_| ())?;
    let mut chunks: Vec<Chunk> = Vec::new();
    media::decode_mono_16khz(&fixture, |samples| {
        chunker.push(samples, &mut |chunk| {
            chunks.push(chunk);
            Ok(())
        })
    })
    .map_err(|_| ())?;
    chunker
        .finish(&mut |chunk| {
            chunks.push(chunk);
            Ok(())
        })
        .map_err(|_| ())?;
    let sample = chunks.into_iter().next().ok_or(())?;
    if sample.duration_ms == 0 || sample.sample_count == 0 || sample.flac_base64.is_empty() {
        return Err(());
    }
    let transcript = transcribe_chunk(
        &gateway,
        model_defaults::DEFAULT_TRANSCRIBE_MODEL,
        &sample,
        TranscribeLanguage::Auto,
        consent,
        CancellationToken::new(),
    )
    .await
    .map_err(|_| ())?;
    if transcript.confirmed_silence || !transcript.segments.is_empty() {
        Ok(())
    } else {
        Err(())
    }
}

fn fail(message: &'static str) -> ExitCode {
    eprintln!("::error::{message}. No key value was printed. If current model IDs were retired, update the defaults and docs/recommended-settings-format.md.");
    ExitCode::FAILURE
}
