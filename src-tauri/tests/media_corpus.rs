use std::path::PathBuf;

use trans_kun_lib::{
    core::error::Category,
    media::{decode_mono_16khz, probe},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/media")
        .join(name)
}

#[test]
fn generated_corpus_probes_and_stream_decodes_all_eleven_supported_formats() {
    // All fixture inputs are synthetic sine/color sources made by the local
    // development-only generator. These tests do not invoke or bundle ffmpeg.
    let supported = [
        ("sample.mp3", "mp3", false),
        ("sample.m4a", "m4a", false),
        ("sample.mp4", "mp4", true),
        ("sample.mov", "mov", true),
        ("sample.mkv", "mkv", true),
        ("sample.webm", "webm", true),
        ("sample.wav", "wav", false),
        ("sample.flac", "flac", false),
        ("sample.ogg", "ogg", false),
        ("sample.aiff", "aiff", false),
        ("sample.caf", "caf", false),
    ];

    for (name, container, has_video) in supported {
        let path = fixture(name);
        let info = probe(&path).unwrap_or_else(|error| panic!("probe {name}: {error}"));
        assert_eq!(info.container, container, "container for {name}");
        assert!(info.frame_count > 0, "frame count for {name}");
        assert!(
            (info.duration_seconds - 1.0).abs() < 0.2,
            "probe duration for {name}: {}s",
            info.duration_seconds
        );
        if has_video {
            assert_eq!(
                info.sample_rate, 48_000,
                "the first (48 kHz) audio track must win for {name}"
            );
        }

        let decoded = decode_mono_16khz(&path, |_| Ok(()))
            .unwrap_or_else(|error| panic!("decode {name}: {error}"));
        assert!(decoded.source_frame_count > 0, "source frames for {name}");
        assert!(decoded.output_frame_count > 0, "output frames for {name}");
        assert!(
            (decoded.duration_seconds - 1.0).abs() < 0.2,
            "decoded duration for {name}: {}s",
            decoded.duration_seconds
        );
    }
}

#[test]
fn opus_in_webm_and_mkv_is_rejected_as_a_format_error() {
    for name in ["opus.webm", "opus.mkv"] {
        let path = fixture(name);
        let probe_error = probe(&path).expect_err(name);
        assert_eq!(probe_error.category, Category::Format, "probe {name}");
        let decode_error = decode_mono_16khz(&path, |_| Ok(())).expect_err(name);
        assert_eq!(decode_error.category, Category::Format, "decode {name}");
        assert!(
            !probe_error
                .detail_redacted
                .contains(path.to_string_lossy().as_ref()),
            "path leaked from probe error for {name}"
        );
    }
}

#[test]
fn unsupported_avi_is_rejected_before_probe_with_a_format_error() {
    let path = fixture("unsupported.avi");
    let error = probe(&path).expect_err("AVI should be unsupported");
    assert_eq!(error.category, Category::Format);
    assert!(error.detail_redacted.contains("mp4, m4a, or mp3"));
    assert!(!error
        .detail_redacted
        .contains(path.to_string_lossy().as_ref()));
}

#[test]
fn asset_protocol_scope_is_limited_to_app_data_media() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(config_path).unwrap()).unwrap();
    let asset_protocol = &config["app"]["security"]["assetProtocol"];

    assert_eq!(asset_protocol["enable"], true);
    assert_eq!(
        asset_protocol["scope"],
        serde_json::json!(["$APPDATA/media/**"])
    );
}
