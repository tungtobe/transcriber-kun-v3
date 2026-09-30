//! House-ad creative selection, display acknowledgement, and local-only
//! impression/action counts. Network access stays behind `remote/`; this
//! module never sends impressions, clicks, or reports to a server.

use std::collections::HashMap;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use chrono::DateTime;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

use crate::remote::{self, ImageMime, RemoteDocument, RemoteSource, VerifiedImage};

pub const FALLBACK_CREATIVE_ID: &str = "embedded:transcriber-kun";
const SELECTED_ACTION_TTL_SECONDS: i64 = 24 * 60 * 60;
const MAX_SELECTED_ACTIONS: usize = 256;

/// Payload inside Story 6.1's signed `ads.json` envelope. Per-creative fields
/// are decoded independently so one malformed creative does not discard
/// otherwise valid candidates. Image references retain the camelCase fields
/// required by `remote::RemoteGateway::fetch_image`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdsManifest {
    pub ads_enabled: bool,
    #[serde(default)]
    pub report_url: Option<String>,
    #[serde(default)]
    pub creatives: Vec<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum AdsLocale {
    Vi,
    En,
    Ja,
}

/// Safe view returned to Story 6.4. URLs and remote creative IDs remain in
/// Rust; the frontend receives only a process-local action token and verified
/// image bytes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AdCreativeView {
    pub token: String,
    pub sponsored_label: String,
    pub sponsor: String,
    pub title: String,
    pub body: String,
    pub image_data_url: Option<String>,
    pub width: u32,
    pub height: u32,
    pub why_this_ad: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AdsImpressionResult {
    Recorded,
    Duplicate,
    UnknownSelection,
    Suppressed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AdsActionResult {
    Opened,
    Unavailable,
    UnknownSelection,
    Suppressed,
}

#[derive(Debug, Clone)]
pub(crate) struct AdCandidate {
    pub manifest_index: usize,
    pub creative_id: String,
    pub locale: AdsLocale,
    pub weight: f64,
    pub sponsor: String,
    pub title: String,
    pub body: String,
    pub click_url: Url,
    pub report_url: Url,
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedAdAction {
    pub creative_id: String,
    pub click_url: Option<Url>,
    pub report_url: Option<Url>,
    selected_at_unix_seconds: i64,
    visible_confirmed: bool,
}

/// Short-lived bridge between `ads_next` and explicit display/click/report
/// commands. The token is random and process-local; a restart invalidates all
/// pending actions without affecting the durable impression cap.
#[derive(Debug, Default)]
pub struct SelectionRegistry {
    selections: Mutex<HashMap<String, SelectedAdAction>>,
}

impl SelectionRegistry {
    pub(crate) fn register(
        &self,
        creative_id: String,
        click_url: Option<Url>,
        report_url: Option<Url>,
        now_unix_seconds: i64,
    ) -> String {
        let token = uuid::Uuid::now_v7().to_string();
        let mut selections = self
            .selections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune_selections(&mut selections, now_unix_seconds);
        if selections.len() >= MAX_SELECTED_ACTIONS {
            if let Some(oldest) = selections
                .iter()
                .min_by_key(|(_, selection)| selection.selected_at_unix_seconds)
                .map(|(token, _)| token.clone())
            {
                selections.remove(&oldest);
            }
        }
        selections.insert(
            token.clone(),
            SelectedAdAction {
                creative_id,
                click_url,
                report_url,
                selected_at_unix_seconds: now_unix_seconds,
                visible_confirmed: false,
            },
        );
        token
    }

    /// Mark the explicit visible-display acknowledgement and return its
    /// action data for the local impression write.
    pub(crate) fn confirm_visible(
        &self,
        token: &str,
        now_unix_seconds: i64,
    ) -> Option<SelectedAdAction> {
        let mut selections = self
            .selections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune_selections(&mut selections, now_unix_seconds);
        let selection = selections.get_mut(token)?;
        selection.visible_confirmed = true;
        Some(selection.clone())
    }

    pub(crate) fn visible_selection(
        &self,
        token: &str,
        now_unix_seconds: i64,
    ) -> Option<SelectedAdAction> {
        let mut selections = self
            .selections
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        prune_selections(&mut selections, now_unix_seconds);
        selections
            .get(token)
            .filter(|selection| selection.visible_confirmed)
            .cloned()
    }
}

fn prune_selections(selections: &mut HashMap<String, SelectedAdAction>, now: i64) {
    selections.retain(|_, selection| {
        now.saturating_sub(selection.selected_at_unix_seconds) < SELECTED_ACTION_TTL_SECONDS
    });
}

/// Only a verified network/cache flag can disable the embedded fallback. An
/// unconfigured or failed remote fetch is represented by an embedded source
/// and therefore still gets the safe bundled creative unless premium is set.
pub fn suppress_ads(is_premium: bool, document: Option<&RemoteDocument<AdsManifest>>) -> bool {
    is_premium
        || document.is_some_and(|document| {
            document.source != RemoteSource::Embedded && !document.payload.ads_enabled
        })
}

/// Parse, locale/time/cap filter, and validate remote candidates. Invalid
/// candidates are skipped without making other signed entries unusable.
pub(crate) fn eligible_candidates(
    manifest: &AdsManifest,
    locale: AdsLocale,
    now_unix_seconds: i64,
    last_displays: &HashMap<String, i64>,
) -> Vec<AdCandidate> {
    manifest
        .creatives
        .iter()
        .enumerate()
        .filter_map(|(index, raw)| {
            let candidate = parse_candidate(index, raw, locale, manifest.report_url.as_deref())?;
            if !creative_is_live(raw, now_unix_seconds)
                || last_displays
                    .get(&candidate.creative_id)
                    .is_some_and(|last| {
                        now_unix_seconds.saturating_sub(*last)
                            < crate::db::repo::ads::IMPRESSION_CAP_SECONDS
                    })
            {
                return None;
            }
            Some(candidate)
        })
        .collect()
}

fn parse_candidate(
    manifest_index: usize,
    raw: &Value,
    requested_locale: AdsLocale,
    manifest_report_url: Option<&str>,
) -> Option<AdCandidate> {
    let object = raw.as_object()?;
    let creative_id = object.get("id")?.as_str()?.trim();
    if creative_id.is_empty()
        || creative_id.len() > 256
        || creative_id.chars().any(char::is_control)
    {
        return None;
    }
    let locales = object.get("locale")?;
    let supports_locale = |value: &str| {
        let parsed = match value {
            "vi" => AdsLocale::Vi,
            "en" => AdsLocale::En,
            "ja" => AdsLocale::Ja,
            _ => return false,
        };
        parsed == requested_locale
    };
    let locale_matches = match locales {
        Value::String(value) => supports_locale(value),
        Value::Array(values) => values
            .iter()
            .any(|value| value.as_str().is_some_and(supports_locale)),
        _ => false,
    };
    if !locale_matches {
        return None;
    }
    let weight = object.get("weight")?.as_f64()?;
    if !weight.is_finite() || weight <= 0.0 {
        return None;
    }
    let title = safe_copy(object.get("title")?.as_str()?, 120)?;
    let sponsor = match object.get("sponsor") {
        Some(value) => safe_copy(value.as_str()?, 80)?,
        None => "Transcriber Kun".to_string(),
    };
    let body = match object.get("body") {
        Some(value) => safe_copy(value.as_str()?, 320)?,
        None => String::new(),
    };
    let click = object
        .get("url")
        .or_else(|| object.get("clickUrl"))
        .and_then(Value::as_str)
        .and_then(|value| remote::validate_click_url(value).ok())?;
    let report = object
        .get("reportUrl")
        .and_then(Value::as_str)
        .or(manifest_report_url)?;
    let report = remote::build_report_url(report, creative_id).ok()?;
    object.get("image")?.as_object()?;

    Some(AdCandidate {
        manifest_index,
        creative_id: creative_id.to_string(),
        locale: requested_locale,
        weight,
        sponsor,
        title,
        body,
        click_url: click,
        report_url: report,
    })
}

fn safe_copy(raw: &str, max_chars: usize) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > max_chars || value.chars().any(char::is_control)
    {
        None
    } else {
        Some(value.to_string())
    }
}

fn creative_is_live(raw: &Value, now: i64) -> bool {
    let Some(object) = raw.as_object() else {
        return false;
    };
    let Some(starts_at) = object
        .get("start")
        .or_else(|| object.get("startAt"))
        .and_then(parse_timestamp)
    else {
        return false;
    };
    let Some(ends_at) = object
        .get("end")
        .or_else(|| object.get("endAt"))
        .and_then(parse_timestamp)
    else {
        return false;
    };
    starts_at <= now && now <= ends_at
}

fn parse_timestamp(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_str()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp())
    })
}

/// Produce a weighted random permutation. The caller tries images in this
/// order and moves on if a candidate fails digest/decode/dimension checks.
pub(crate) fn weighted_order(
    mut candidates: Vec<AdCandidate>,
    mut next_unit_sample: impl FnMut() -> f64,
) -> Vec<AdCandidate> {
    let mut ordered = Vec::with_capacity(candidates.len());
    while !candidates.is_empty() {
        let max_weight = candidates
            .iter()
            .map(|candidate| candidate.weight)
            .fold(0.0_f64, f64::max);
        if !max_weight.is_finite() || max_weight <= 0.0 {
            break;
        }
        let normalized_total = candidates
            .iter()
            .map(|candidate| candidate.weight / max_weight)
            .sum::<f64>();
        if !normalized_total.is_finite() || normalized_total <= 0.0 {
            break;
        }

        let sample = next_unit_sample();
        let unit = if sample.is_finite() {
            sample.clamp(0.0, 1.0 - f64::EPSILON)
        } else {
            0.0
        };
        let target = unit * normalized_total;
        let mut accumulated = 0.0;
        let selected_index = candidates
            .iter()
            .position(|candidate| {
                accumulated += candidate.weight / max_weight;
                target < accumulated
            })
            .unwrap_or(candidates.len() - 1);
        ordered.push(candidates.remove(selected_index));
    }
    ordered
}

pub(crate) fn display_dimensions_allowed(width: u32, height: u32) -> bool {
    matches!((width, height), (300, 100) | (320, 50))
}

pub(crate) fn image_data_url(image: &VerifiedImage) -> String {
    let mime = match image.mime {
        ImageMime::Png => "image/png",
        ImageMime::Jpeg => "image/jpeg",
        ImageMime::Webp => "image/webp",
    };
    format!("data:{mime};base64,{}", BASE64.encode(&image.bytes))
}

pub(crate) fn fallback_view(
    locale: AdsLocale,
    selections: &SelectionRegistry,
    now_unix_seconds: i64,
) -> AdCreativeView {
    let (sponsored_label, title, body, why_this_ad) = match locale {
        AdsLocale::En => (
            "Sponsored",
            "Made for focused conversations",
            "Transcribe and organize meetings with Transcriber Kun.",
            "Shown based on your interface language and the current ad schedule. No personal data is used.",
        ),
        AdsLocale::Vi => (
            "Tài trợ",
            "Tập trung vào cuộc trò chuyện",
            "Chép lời và sắp xếp cuộc họp với Transcriber Kun.",
            "Quảng cáo được chọn theo ngôn ngữ giao diện và lịch hiển thị. Không dùng dữ liệu cá nhân.",
        ),
        AdsLocale::Ja => (
            "スポンサー",
            "会話に集中するために",
            "Transcriber Kun で会議を文字起こしして整理しましょう。",
            "表示言語と広告スケジュールに基づいて表示しています。個人データは使用しません。",
        ),
    };
    let token = selections.register(
        FALLBACK_CREATIVE_ID.to_string(),
        None,
        None,
        now_unix_seconds,
    );
    AdCreativeView {
        token,
        sponsored_label: sponsored_label.to_string(),
        sponsor: "Transcriber Kun".to_string(),
        title: title.to_string(),
        body: body.to_string(),
        image_data_url: None,
        width: 300,
        height: 100,
        why_this_ad: why_this_ad.to_string(),
    }
}

pub(crate) fn remote_view(
    candidate: AdCandidate,
    image: &VerifiedImage,
    selections: &SelectionRegistry,
    now_unix_seconds: i64,
) -> AdCreativeView {
    let token = selections.register(
        candidate.creative_id,
        Some(candidate.click_url),
        Some(candidate.report_url),
        now_unix_seconds,
    );
    let (sponsored_label, why_this_ad) = match candidate.locale {
        AdsLocale::En => (
            "Sponsored",
            "Shown based on your interface language and the current ad schedule. No personal data is used.",
        ),
        AdsLocale::Vi => (
            "Tài trợ",
            "Quảng cáo được chọn theo ngôn ngữ giao diện và lịch hiển thị. Không dùng dữ liệu cá nhân.",
        ),
        AdsLocale::Ja => (
            "スポンサー",
            "表示言語と広告スケジュールに基づいて表示しています。個人データは使用しません。",
        ),
    };
    AdCreativeView {
        token,
        sponsored_label: sponsored_label.to_string(),
        sponsor: candidate.sponsor,
        title: candidate.title,
        body: candidate.body,
        image_data_url: Some(image_data_url(image)),
        width: image.width,
        height: image.height,
        why_this_ad: why_this_ad.to_string(),
    }
}

/// Current Unix time for local cap/action persistence. Clock injection is
/// done at the pure selection/repository boundaries for deterministic tests.
pub fn now_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
}

pub fn random_unit_sample() -> f64 {
    rand::random::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{repo, Db};
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn raw_creative(
        id: &str,
        locale: &str,
        start_at: Value,
        end_at: Value,
        weight: Value,
    ) -> Value {
        serde_json::json!({
            "id": id,
            "locale": locale,
            "startAt": start_at,
            "endAt": end_at,
            "weight": weight,
            "title": format!("Ad {id}"),
            "body": "A short description",
            "clickUrl": "https://ads.example/offer",
            "image": {
                "url": "https://ads.example/image.png",
                "sha256": "00".repeat(32),
                "mimeType": "image/png"
            }
        })
    }

    fn manifest(creatives: Vec<Value>) -> AdsManifest {
        AdsManifest {
            ads_enabled: true,
            report_url: Some("https://ads.example/report".to_string()),
            creatives,
        }
    }

    fn open_db() -> Db {
        let dir = tempdir().unwrap();
        Db::open(&dir.keep()).unwrap()
    }

    #[test]
    fn eligible_signed_candidates_match_locale_and_inclusive_start_end_times() {
        let manifest = manifest(vec![
            raw_creative("starts-now", "en", 1_000.into(), 2_000.into(), 1.into()),
            raw_creative("ends-now", "en", 500.into(), 1_000.into(), 1.into()),
            raw_creative("wrong-locale", "ja", 500.into(), 2_000.into(), 1.into()),
            raw_creative("not-started", "en", 1_001.into(), 2_000.into(), 1.into()),
            raw_creative("expired", "en", 500.into(), 999.into(), 1.into()),
        ]);

        let candidates = eligible_candidates(&manifest, AdsLocale::En, 1_000, &HashMap::new());
        let ids = candidates
            .into_iter()
            .map(|candidate| candidate.creative_id)
            .collect::<Vec<_>>();
        assert_eq!(ids, ["starts-now", "ends-now"]);
    }

    #[test]
    fn invalid_weights_and_recently_displayed_candidates_are_skipped() {
        let manifest = manifest(vec![
            raw_creative("zero", "en", 0.into(), 2_000.into(), 0.into()),
            raw_creative("negative", "en", 0.into(), 2_000.into(), (-1).into()),
            raw_creative("recent", "en", 0.into(), 2_000.into(), 1.into()),
            raw_creative("eligible", "en", 0.into(), 2_000.into(), 1.into()),
        ]);
        let history = HashMap::from([("recent".to_string(), 999)]);
        let candidates = eligible_candidates(&manifest, AdsLocale::En, 1_000, &history);
        assert_eq!(
            candidates
                .iter()
                .map(|candidate| candidate.creative_id.as_str())
                .collect::<Vec<_>>(),
            ["eligible"]
        );
    }

    #[test]
    fn weighted_order_uses_injected_randomness_and_preserves_weight_proportions() {
        let manifest = manifest(vec![
            raw_creative("light", "en", 0.into(), 2_000.into(), 1.into()),
            raw_creative("heavy", "en", 0.into(), 2_000.into(), 9.into()),
        ]);
        let candidates = eligible_candidates(&manifest, AdsLocale::En, 1_000, &HashMap::new());
        let mut samples = [0.95, 0.5].into_iter();
        let order = weighted_order(candidates, || samples.next().unwrap());
        assert_eq!(order[0].creative_id, "heavy");
        assert_eq!(order[1].creative_id, "light");
    }

    #[test]
    fn disabled_signed_manifest_and_local_premium_suppress_even_fallback() {
        let disabled = remote::test_fresh_document(AdsManifest {
            ads_enabled: false,
            ..Default::default()
        });
        assert!(suppress_ads(false, Some(&disabled)));
        assert!(suppress_ads(true, None));

        let fallback_source = remote::test_embedded_document(
            AdsManifest {
                ads_enabled: true,
                ..Default::default()
            },
            Some(remote::RemoteFailure::Network),
        );
        assert!(!suppress_ads(false, Some(&fallback_source)));
    }

    #[test]
    fn fallback_is_bundled_localized_and_has_stable_dimensions() {
        let selections = SelectionRegistry::default();
        for (locale, label) in [
            (AdsLocale::Vi, "Tài trợ"),
            (AdsLocale::En, "Sponsored"),
            (AdsLocale::Ja, "スポンサー"),
        ] {
            let view = fallback_view(locale, &selections, 1_000);
            assert_eq!(view.sponsored_label, label);
            assert_eq!((view.width, view.height), (300, 100));
            assert!(view.image_data_url.is_none());
            assert!(!view.title.is_empty());
            assert!(!view.body.is_empty());
            assert!(!view.why_this_ad.is_empty());
        }
    }

    #[test]
    fn only_supported_remote_image_sizes_are_displayable() {
        assert!(display_dimensions_allowed(300, 100));
        assert!(display_dimensions_allowed(320, 50));
        assert!(!display_dimensions_allowed(301, 100));
        assert!(!display_dimensions_allowed(300, 101));
    }

    #[test]
    fn impressions_require_explicit_confirmation_and_fallback_remounts_do_not_recount() {
        let db = open_db();
        let selections = SelectionRegistry::default();
        let first = fallback_view(AdsLocale::En, &selections, 1_000);
        assert!(selections.visible_selection(&first.token, 1_000).is_none());
        let selected = selections
            .confirm_visible(&first.token, 1_000)
            .expect("visible display ack");
        let first_recorded = db
            .with_connection(|conn| {
                Ok(repo::ads::record_impression(
                    conn,
                    &selected.creative_id,
                    1_000,
                )?)
            })
            .unwrap();
        assert!(first_recorded);

        let remount = fallback_view(AdsLocale::En, &selections, 1_005);
        let selected = selections
            .confirm_visible(&remount.token, 1_005)
            .expect("remounted fallback visible ack");
        let second_recorded = db
            .with_connection(|conn| {
                Ok(repo::ads::record_impression(
                    conn,
                    &selected.creative_id,
                    1_005,
                )?)
            })
            .unwrap();
        assert!(!second_recorded);
    }

    #[test]
    fn selection_token_does_not_expose_urls_and_actions_require_visible_ack() {
        let selections = SelectionRegistry::default();
        let candidate = eligible_candidates(
            &manifest(vec![raw_creative(
                "remote-id",
                "en",
                0.into(),
                2_000.into(),
                1.into(),
            )]),
            AdsLocale::En,
            1_000,
            &HashMap::new(),
        )
        .pop()
        .unwrap();
        let token = selections.register(
            candidate.creative_id,
            Some(candidate.click_url),
            Some(candidate.report_url),
            1_000,
        );
        assert!(selections.visible_selection(&token, 1_000).is_none());
        let visible = selections.confirm_visible(&token, 1_000).unwrap();
        assert_eq!(
            visible.click_url.unwrap().as_str(),
            "https://ads.example/offer"
        );
        assert!(visible
            .report_url
            .unwrap()
            .as_str()
            .contains("creative_id=remote-id"));
        assert!(!token.contains("ads.example"));
    }

    #[test]
    fn embedded_source_after_remote_failure_uses_fallback() {
        let selections = SelectionRegistry::default();
        let view = fallback_view(AdsLocale::Ja, &selections, 1_800_000_000);
        let selected = selections
            .confirm_visible(&view.token, 1_800_000_000)
            .unwrap();
        assert_eq!(selected.creative_id, FALLBACK_CREATIVE_ID);
        assert!(selected.click_url.is_none());
        assert!(selected.report_url.is_none());
    }
}
