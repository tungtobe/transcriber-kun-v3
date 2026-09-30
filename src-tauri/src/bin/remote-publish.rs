use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::SigningKey;
use image::ImageFormat;
use reqwest::Url;
use serde_json::Value;
use trans_kun_lib::{ads, remote, settings::recommended};

const HELP: &str = "\
remote-publish build --kind <ads|recommended-settings> --input <payload.json>
  --key <external-private-key> --origin <https-origin>
  --privacy-policy-url <https-url> --key-custodian <name> --out <bundle-dir>
  [--image <creative-id>=<path> ...]

remote-publish verify --kind <ads|recommended-settings> --input <envelope.json>
  --public-key-base64 <base64-public-key> [--origin <https-origin>]
  [--image <creative-id>=<path> ...]

remote-publish public-key --key <external-private-key>

Private keys are accepted only from a file outside the repository. Key files
must contain a 32-byte seed as 64 hexadecimal characters or base64.
";

#[derive(Clone, Copy)]
enum Kind {
    Ads,
    RecommendedSettings,
}

struct Options {
    values: HashMap<String, String>,
    images: Vec<(String, PathBuf)>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("remote-publish: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), &'static str> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or(HELP)?;
    let rest = args.collect::<Vec<_>>();
    match command.as_str() {
        "build" => build_bundle(parse_options(&rest)?)?,
        "verify" => verify_input(parse_options(&rest)?)?,
        "public-key" => print_public_key(parse_options(&rest)?)?,
        "help" | "--help" | "-h" => print!("{HELP}"),
        _ => return Err(HELP),
    }
    Ok(())
}

fn parse_options(args: &[String]) -> Result<Options, &'static str> {
    let mut values = HashMap::new();
    let mut images = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let name = args[index].as_str();
        if name == "--image" {
            let value = args.get(index + 1).ok_or("--image needs ID=PATH")?;
            let (id, path) = value.split_once('=').ok_or("--image needs ID=PATH")?;
            if id.trim().is_empty() || path.trim().is_empty() {
                return Err("--image needs ID=PATH");
            }
            images.push((id.to_owned(), PathBuf::from(path)));
            index += 2;
            continue;
        }
        if !name.starts_with("--") {
            return Err("unexpected argument; run remote-publish help");
        }
        let value = args.get(index + 1).ok_or("option is missing a value")?;
        if values.insert(name.to_owned(), value.to_owned()).is_some() {
            return Err("an option was provided more than once");
        }
        index += 2;
    }
    Ok(Options { values, images })
}

impl Options {
    fn required(&self, name: &'static str) -> Result<&str, &'static str> {
        self.values
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("a required option is missing; run remote-publish help")
    }

    fn optional(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }
}

fn parse_kind(value: &str) -> Result<Kind, &'static str> {
    match value {
        "ads" => Ok(Kind::Ads),
        "recommended-settings" => Ok(Kind::RecommendedSettings),
        _ => Err("--kind must be ads or recommended-settings"),
    }
}

fn build_bundle(options: Options) -> Result<(), &'static str> {
    validate_option_names(
        &options,
        &[
            "--kind",
            "--input",
            "--key",
            "--origin",
            "--privacy-policy-url",
            "--key-custodian",
            "--out",
        ],
    )?;
    let kind = parse_kind(options.required("--kind")?)?;
    let input = read_json(options.required("--input")?)?;
    let signing_key = load_signing_key(Path::new(options.required("--key")?))?;
    let origin = parse_origin(options.required("--origin")?)?;
    let privacy_url = parse_https_url(options.required("--privacy-policy-url")?)?;
    let custodian = options.required("--key-custodian")?;
    if custodian.trim().chars().count() > 200 || custodian.chars().any(char::is_control) {
        return Err("--key-custodian must be a short, printable name or team");
    }
    let output = PathBuf::from(options.required("--out")?);

    let mut files = BTreeMap::<PathBuf, Vec<u8>>::new();
    let payload = match kind {
        Kind::Ads => build_ads_payload(input, &origin, &options.images, &mut files)?,
        Kind::RecommendedSettings => {
            if !options.images.is_empty() {
                return Err("--image is only valid for ads bundles");
            }
            let document: recommended::RecommendedSettingsDocument = serde_json::from_value(input)
                .map_err(|_| "recommended-settings payload is invalid")?;
            let document = recommended::validate_publication_document(document)
                .map_err(|_| "recommended-settings payload failed validation")?;
            serde_json::to_value(document).map_err(|_| "recommended-settings payload is invalid")?
        }
    };

    let envelope = remote::sign_envelope(payload, &signing_key)
        .map_err(|_| "could not create the signed envelope")?;
    verify_envelope_for_kind(&envelope, kind, &signing_key.verifying_key())?;
    files.insert(
        PathBuf::from(match kind {
            Kind::Ads => "ads.json",
            Kind::RecommendedSettings => "recommended-settings.json",
        }),
        envelope,
    );

    write_bundle(&output, files)?;

    println!("Signed bundle created and verified.");
    println!(
        "TRANS_KUN_REMOTE_ORIGIN={}",
        origin.as_str().trim_end_matches('/')
    );
    println!(
        "TRANS_KUN_REMOTE_ED25519_PUBLIC_KEY_B64={}",
        BASE64.encode(signing_key.verifying_key().to_bytes())
    );
    println!("TRANS_KUN_PRIVACY_POLICY_URL={}", privacy_url.as_str());
    println!("Key custodian supplied; record it in the private key-custody register.");
    Ok(())
}

fn build_ads_payload(
    mut payload: Value,
    origin: &Url,
    image_args: &[(String, PathBuf)],
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<Value, &'static str> {
    let creatives = payload
        .get_mut("creatives")
        .and_then(Value::as_array_mut)
        .ok_or("ads payload must contain a creatives array")?;
    let mut images = HashMap::new();
    for (id, path) in image_args {
        if images.insert(id.as_str(), path).is_some() {
            return Err("an image was provided more than once for a creative");
        }
    }
    let mut creative_ids = HashSet::new();
    for creative in creatives {
        let object = creative
            .as_object_mut()
            .ok_or("every creative must be an object")?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or("every creative needs an ID")?;
        if !creative_ids.insert(id.to_owned()) {
            return Err("creative IDs must be unique");
        }
        let path = images
            .remove(id)
            .ok_or("every creative needs one --image ID=PATH")?;
        let bytes = read_creative_image(path)?;
        let format =
            image::guess_format(&bytes).map_err(|_| "creative image format is unsupported")?;
        let (mime, extension) = mime_and_extension(format)?;
        let validated = remote::validate_publication_image(bytes, mime)
            .map_err(|_| "creative image failed MIME, digest, size, or decode validation")?;
        if !ads::display_dimensions_allowed(validated.width, validated.height) {
            return Err("creative image must be 300x100 or 320x50 pixels");
        }
        let digest = hex(&validated.sha256);
        let filename = format!("{digest}.{extension}");
        let image_url = origin
            .join(&format!("images/{filename}"))
            .map_err(|_| "could not construct a creative image URL")?;
        object.insert(
            "image".to_owned(),
            serde_json::json!({
                "url": image_url.as_str(),
                "sha256": digest,
                "mimeType": mime,
            }),
        );
        files.insert(PathBuf::from("images").join(filename), validated.bytes);
    }
    if !images.is_empty() {
        return Err("an --image ID does not match an ads creative");
    }

    let manifest: ads::AdsManifest =
        serde_json::from_value(payload.clone()).map_err(|_| "ads payload schema is invalid")?;
    ads::validate_publication_manifest(&manifest).map_err(|_| "ads creative failed validation")?;
    for creative in &manifest.creatives {
        let image = creative
            .get("image")
            .and_then(Value::as_object)
            .ok_or("creative image reference is invalid")?;
        let url = image
            .get("url")
            .and_then(Value::as_str)
            .ok_or("creative image URL is invalid")?;
        if parse_https_url(url)?.origin() != origin.origin() {
            return Err("creative image URL must use the configured Pages origin");
        }
    }
    Ok(payload)
}

fn verify_input(options: Options) -> Result<(), &'static str> {
    validate_option_names(
        &options,
        &["--kind", "--input", "--public-key-base64", "--origin"],
    )?;
    let kind = parse_kind(options.required("--kind")?)?;
    let bytes =
        fs::read(options.required("--input")?).map_err(|_| "could not read the signed envelope")?;
    let key_bytes = BASE64
        .decode(options.required("--public-key-base64")?)
        .map_err(|_| "public key must be base64")?;
    let key_array: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "public key must decode to 32 bytes")?;
    let verifying_key =
        ed25519_dalek::VerifyingKey::from_bytes(&key_array).map_err(|_| "public key is invalid")?;
    verify_envelope_for_kind(&bytes, kind, &verifying_key)?;

    if matches!(kind, Kind::Ads) {
        let origin = parse_origin(options.required("--origin")?)?;
        verify_ads_images(&bytes, &verifying_key, &origin, &options.images)?;
    } else if !options.images.is_empty() || options.optional("--origin").is_some() {
        return Err("--origin and --image are only valid when verifying ads");
    }
    println!("Envelope and publication content verified.");
    Ok(())
}

fn verify_envelope_for_kind(
    bytes: &[u8],
    kind: Kind,
    key: &ed25519_dalek::VerifyingKey,
) -> Result<(), &'static str> {
    match kind {
        Kind::Ads => {
            let (manifest, _): (ads::AdsManifest, _) =
                remote::verify_envelope_with_digest(bytes, key)
                    .map_err(|_| "signed ads envelope failed client verification")?;
            ads::validate_publication_manifest(&manifest)
                .map_err(|_| "signed ads manifest failed validation")?;
        }
        Kind::RecommendedSettings => {
            let (document, _): (recommended::RecommendedSettingsDocument, _) =
                remote::verify_envelope_with_digest(bytes, key)
                    .map_err(|_| "signed recommendations failed client verification")?;
            recommended::validate_publication_document(document)
                .map_err(|_| "signed recommendations failed validation")?;
        }
    }
    Ok(())
}

fn verify_ads_images(
    bytes: &[u8],
    key: &ed25519_dalek::VerifyingKey,
    origin: &Url,
    image_args: &[(String, PathBuf)],
) -> Result<(), &'static str> {
    let (manifest, _): (ads::AdsManifest, _) = remote::verify_envelope_with_digest(bytes, key)
        .map_err(|_| "signed ads envelope failed client verification")?;
    let mut images = HashMap::new();
    for (id, path) in image_args {
        if images.insert(id.as_str(), path).is_some() {
            return Err("an image was provided more than once for a creative");
        }
    }
    let mut seen = HashSet::new();
    for creative in &manifest.creatives {
        let id = creative
            .get("id")
            .and_then(Value::as_str)
            .ok_or("signed creative ID is invalid")?;
        if !seen.insert(id) {
            return Err("signed creative IDs must be unique");
        }
        let image = creative
            .get("image")
            .and_then(Value::as_object)
            .ok_or("signed creative image reference is invalid")?;
        let url = image
            .get("url")
            .and_then(Value::as_str)
            .ok_or("signed creative image URL is invalid")?;
        let digest = image
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or("signed creative image digest is invalid")?;
        let mime = image
            .get("mimeType")
            .and_then(Value::as_str)
            .ok_or("signed creative image MIME type is invalid")?;
        let path = images
            .remove(id)
            .ok_or("every signed creative needs its local image")?;
        let bytes = read_creative_image(path)?;
        let validated = remote::validate_publication_image(bytes, mime)
            .map_err(|_| "creative image failed MIME, digest, size, or decode validation")?;
        if hex(&validated.sha256) != digest {
            return Err("creative image digest does not match the signed manifest");
        }
        if !ads::display_dimensions_allowed(validated.width, validated.height) {
            return Err("creative image must be 300x100 or 320x50 pixels");
        }
        let extension = extension_for_mime(validated.mime)?.1;
        let expected_file = format!("images/{}.{}", digest, extension);
        let expected_url = origin
            .join(&expected_file)
            .map_err(|_| "creative image URL is invalid")?;
        if Url::parse(url)
            .map_err(|_| "creative image URL is invalid")?
            .as_str()
            != expected_url.as_str()
        {
            return Err("creative image URL does not match the Pages bundle path");
        }
    }
    if !images.is_empty() {
        return Err("an --image ID does not match a signed creative");
    }
    Ok(())
}

fn print_public_key(options: Options) -> Result<(), &'static str> {
    validate_option_names(&options, &["--key"])?;
    let key = load_signing_key(Path::new(options.required("--key")?))?;
    println!(
        "TRANS_KUN_REMOTE_ED25519_PUBLIC_KEY_B64={}",
        BASE64.encode(key.verifying_key().to_bytes())
    );
    Ok(())
}

fn validate_option_names(options: &Options, allowed: &[&str]) -> Result<(), &'static str> {
    if options
        .values
        .keys()
        .any(|name| !allowed.contains(&name.as_str()))
    {
        return Err("unknown option; run remote-publish help");
    }
    Ok(())
}

fn load_signing_key(path: &Path) -> Result<SigningKey, &'static str> {
    let canonical_path = path
        .canonicalize()
        .map_err(|_| "could not access the signing key file")?;
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("could not determine the repository root")?
        .canonicalize()
        .map_err(|_| "could not determine the repository root")?;
    if canonical_path.starts_with(repository_root) {
        return Err("the signing key must be stored outside the repository");
    }
    let contents = fs::read(canonical_path).map_err(|_| "could not read the signing key file")?;
    let trimmed = trim_ascii_whitespace(&contents);
    let seed = if trimmed.len() == 64 && trimmed.iter().all(u8::is_ascii_hexdigit) {
        decode_hex(trimmed).ok_or("signing key file must contain a 32-byte seed")?
    } else {
        BASE64
            .decode(trimmed)
            .map_err(|_| "signing key file must contain a 32-byte seed")?
    };
    let seed: [u8; 32] = seed
        .as_slice()
        .try_into()
        .map_err(|_| "signing key file must contain a 32-byte seed")?;
    Ok(SigningKey::from_bytes(&seed))
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index + 1);
    &bytes[start..end]
}

fn decode_hex(bytes: &[u8]) -> Option<Vec<u8>> {
    bytes
        .chunks_exact(2)
        .map(|pair| Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte.to_ascii_lowercase() {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte.to_ascii_lowercase() - b'a' + 10),
        _ => None,
    }
}

fn parse_origin(raw: &str) -> Result<Url, &'static str> {
    let origin = Url::parse(raw).map_err(|_| "--origin must be an HTTPS origin")?;
    if origin.scheme() != "https"
        || origin.host_str().is_none()
        || !origin.username().is_empty()
        || origin.password().is_some()
        || origin.path() != "/"
        || origin.query().is_some()
        || origin.fragment().is_some()
    {
        return Err("--origin must be an HTTPS origin without a path or credentials");
    }
    Ok(origin)
}

fn parse_https_url(raw: &str) -> Result<Url, &'static str> {
    let url = Url::parse(raw).map_err(|_| "Privacy Policy URL must use HTTPS")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Privacy Policy URL must use HTTPS without credentials");
    }
    Ok(url)
}

fn read_json(path: &str) -> Result<Value, &'static str> {
    let bytes = fs::read(path).map_err(|_| "could not read the JSON payload")?;
    serde_json::from_slice(&bytes).map_err(|_| "payload is not valid JSON")
}

fn read_creative_image(path: &Path) -> Result<Vec<u8>, &'static str> {
    let size = fs::metadata(path)
        .map_err(|_| "could not access a creative image")?
        .len();
    if size > remote::MAX_PUBLICATION_IMAGE_BYTES as u64 {
        return Err("creative image exceeds the 100 KiB limit");
    }
    let bytes = fs::read(path).map_err(|_| "could not read a creative image")?;
    if bytes.len() > remote::MAX_PUBLICATION_IMAGE_BYTES {
        return Err("creative image exceeds the 100 KiB limit");
    }
    Ok(bytes)
}

fn mime_and_extension(format: ImageFormat) -> Result<(&'static str, &'static str), &'static str> {
    match format {
        ImageFormat::Png => Ok(("image/png", "png")),
        ImageFormat::Jpeg => Ok(("image/jpeg", "jpg")),
        ImageFormat::WebP => Ok(("image/webp", "webp")),
        _ => Err("creative image format must be PNG, JPEG, or WebP"),
    }
}

fn extension_for_mime(
    mime: remote::ImageMime,
) -> Result<(&'static str, &'static str), &'static str> {
    match mime {
        remote::ImageMime::Png => Ok(("image/png", "png")),
        remote::ImageMime::Jpeg => Ok(("image/jpeg", "jpg")),
        remote::ImageMime::Webp => Ok(("image/webp", "webp")),
    }
}

fn write_bundle(output: &Path, files: BTreeMap<PathBuf, Vec<u8>>) -> Result<(), &'static str> {
    if output.exists() {
        return Err("output directory already exists; choose a new path");
    }
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| "could not create the output parent directory")?;
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("output directory name is invalid")?;
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::now_v7()));
    fs::create_dir(&temporary).map_err(|_| "could not create a temporary bundle")?;
    let write_result = (|| {
        for (relative, bytes) in files {
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
            {
                return Err("bundle path is invalid");
            }
            let destination = temporary.join(relative);
            if let Some(directory) = destination.parent() {
                fs::create_dir_all(directory).map_err(|_| "could not create a bundle directory")?;
            }
            fs::write(destination, bytes).map_err(|_| "could not write a bundle file")?;
        }
        fs::rename(&temporary, output).map_err(|_| "could not finalize the bundle")
    })();
    if write_result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    write_result
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use image::{ImageBuffer, Rgb};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[17; 32])
    }

    fn temp_key_file() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("trans-kun-signing-{}.key", uuid::Uuid::now_v7()));
        fs::write(&path, hex(&[17; 32])).unwrap();
        path
    }

    fn test_image(path: &Path) {
        let image = ImageBuffer::from_pixel(300, 100, Rgb([30_u8, 60, 90]));
        image.save(path).unwrap();
    }

    #[test]
    fn generated_envelope_is_deterministic_and_accepted_by_client_verifier() {
        let payload = serde_json::json!({"schemaVersion":1,"settings":{"chunkMinutes":5}});
        let first = remote::sign_envelope(payload.clone(), &key()).unwrap();
        let second = remote::sign_envelope(payload, &key()).unwrap();
        assert_eq!(first, second);
        verify_envelope_for_kind(&first, Kind::RecommendedSettings, &key().verifying_key())
            .unwrap();

        let mut envelope: Value = serde_json::from_slice(&first).unwrap();
        envelope["payload"]["settings"]["chunkMinutes"] = 6.into();
        let changed = serde_json::to_vec(&envelope).unwrap();
        assert_eq!(
            remote::verify_envelope_with_digest::<recommended::RecommendedSettingsDocument>(
                &changed,
                &key().verifying_key()
            )
            .unwrap_err(),
            remote::RemoteFailure::InvalidSignature
        );
    }

    #[test]
    fn build_adds_signed_image_digest_and_verify_rejects_changed_image_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("creative.png");
        test_image(&image_path);
        let payload = serde_json::json!({
            "adsEnabled": true,
            "reportUrl": "https://ads.example/report",
            "creatives": [{
                "id": "demo",
                "locale": ["en", "ja"],
                "startAt": "2026-01-01T00:00:00Z",
                "endAt": "2030-01-01T00:00:00Z",
                "weight": 1.0,
                "title": "Demo",
                "clickUrl": "https://ads.example/offer"
            }]
        });
        let mut files = BTreeMap::new();
        let payload = build_ads_payload(
            payload,
            &parse_origin("https://pages.example").unwrap(),
            &[("demo".to_owned(), image_path.clone())],
            &mut files,
        )
        .unwrap();
        let envelope = remote::sign_envelope(payload, &key()).unwrap();
        verify_ads_images(
            &envelope,
            &key().verifying_key(),
            &parse_origin("https://pages.example").unwrap(),
            &[("demo".to_owned(), image_path.clone())],
        )
        .unwrap();

        let changed_path = dir.path().join("changed.png");
        ImageBuffer::from_pixel(300, 100, Rgb([90_u8, 60, 30]))
            .save(&changed_path)
            .unwrap();
        assert!(verify_ads_images(
            &envelope,
            &key().verifying_key(),
            &parse_origin("https://pages.example").unwrap(),
            &[("demo".to_owned(), changed_path)],
        )
        .is_err());
        assert!(files.keys().any(|path| path.starts_with("images")));
    }

    #[test]
    fn invalid_schema_and_oversized_image_leave_no_output_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let key_path = temp_key_file();

        let settings_input = dir.path().join("invalid-settings.json");
        fs::write(
            &settings_input,
            r#"{"schemaVersion":1,"settings":{"chunkMinutes":61}}"#,
        )
        .unwrap();
        let settings_output = dir.path().join("settings-bundle");
        let settings_options = Options {
            values: HashMap::from([
                ("--kind".to_owned(), "recommended-settings".to_owned()),
                (
                    "--input".to_owned(),
                    settings_input.to_string_lossy().into_owned(),
                ),
                ("--key".to_owned(), key_path.to_string_lossy().into_owned()),
                ("--origin".to_owned(), "https://pages.example".to_owned()),
                (
                    "--privacy-policy-url".to_owned(),
                    "https://pages.example/privacy".to_owned(),
                ),
                ("--key-custodian".to_owned(), "Release Team".to_owned()),
                (
                    "--out".to_owned(),
                    settings_output.to_string_lossy().into_owned(),
                ),
            ]),
            images: Vec::new(),
        };
        assert!(build_bundle(settings_options).is_err());
        assert!(!settings_output.exists());

        let oversize_image = dir.path().join("oversize.png");
        fs::write(
            &oversize_image,
            vec![0_u8; remote::MAX_PUBLICATION_IMAGE_BYTES + 1],
        )
        .unwrap();
        let ads_input = dir.path().join("ads.json");
        fs::write(
            &ads_input,
            r#"{"adsEnabled":true,"creatives":[{"id":"demo"}]}"#,
        )
        .unwrap();
        let ads_output = dir.path().join("ads-bundle");
        let ads_options = Options {
            values: HashMap::from([
                ("--kind".to_owned(), "ads".to_owned()),
                (
                    "--input".to_owned(),
                    ads_input.to_string_lossy().into_owned(),
                ),
                ("--key".to_owned(), key_path.to_string_lossy().into_owned()),
                ("--origin".to_owned(), "https://pages.example".to_owned()),
                (
                    "--privacy-policy-url".to_owned(),
                    "https://pages.example/privacy".to_owned(),
                ),
                ("--key-custodian".to_owned(), "Release Team".to_owned()),
                (
                    "--out".to_owned(),
                    ads_output.to_string_lossy().into_owned(),
                ),
            ]),
            images: vec![("demo".to_owned(), oversize_image)],
        };
        assert!(build_bundle(ads_options).is_err());
        assert!(!ads_output.exists());

        let valid_sized_image = dir.path().join("valid-sized.png");
        test_image(&valid_sized_image);
        let invalid_ads_input = dir.path().join("invalid-ads.json");
        fs::write(
            &invalid_ads_input,
            r#"{"adsEnabled":true,"creatives":[{"id":"demo","locale":"en","startAt":"2026-01-01T00:00:00Z","endAt":"2030-01-01T00:00:00Z","weight":1.0,"clickUrl":"https://ads.example/offer"}]}"#,
        )
        .unwrap();
        let invalid_ads_output = dir.path().join("invalid-ads-bundle");
        let invalid_ads_options = Options {
            values: HashMap::from([
                ("--kind".to_owned(), "ads".to_owned()),
                (
                    "--input".to_owned(),
                    invalid_ads_input.to_string_lossy().into_owned(),
                ),
                ("--key".to_owned(), key_path.to_string_lossy().into_owned()),
                ("--origin".to_owned(), "https://pages.example".to_owned()),
                (
                    "--privacy-policy-url".to_owned(),
                    "https://pages.example/privacy".to_owned(),
                ),
                ("--key-custodian".to_owned(), "Release Team".to_owned()),
                (
                    "--out".to_owned(),
                    invalid_ads_output.to_string_lossy().into_owned(),
                ),
            ]),
            images: vec![("demo".to_owned(), valid_sized_image)],
        };
        assert!(build_bundle(invalid_ads_options).is_err());
        assert!(!invalid_ads_output.exists());
        fs::remove_file(key_path).unwrap();
    }

    #[test]
    fn key_file_must_be_outside_the_repository() {
        let key_path = temp_key_file();
        let loaded = load_signing_key(&key_path).unwrap();
        assert_eq!(loaded.verifying_key(), key().verifying_key());
        fs::remove_file(key_path).unwrap();

        let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert_eq!(
            load_signing_key(&inside).unwrap_err(),
            "the signing key must be stored outside the repository"
        );
    }

    #[test]
    fn repository_key_path_cannot_create_a_publishable_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("settings.json");
        fs::write(
            &input,
            r#"{"schemaVersion":1,"settings":{"chunkMinutes":5}}"#,
        )
        .unwrap();
        let output = dir.path().join("bundle");
        let options = Options {
            values: HashMap::from([
                ("--kind".to_owned(), "recommended-settings".to_owned()),
                ("--input".to_owned(), input.to_string_lossy().into_owned()),
                (
                    "--key".to_owned(),
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("Cargo.toml")
                        .to_string_lossy()
                        .into_owned(),
                ),
                ("--origin".to_owned(), "https://pages.example".to_owned()),
                (
                    "--privacy-policy-url".to_owned(),
                    "https://pages.example/privacy".to_owned(),
                ),
                ("--key-custodian".to_owned(), "Release Team".to_owned()),
                ("--out".to_owned(), output.to_string_lossy().into_owned()),
            ]),
            images: Vec::new(),
        };
        let error = build_bundle(options).unwrap_err();
        assert_eq!(
            error,
            "the signing key must be stored outside the repository"
        );
        assert!(!output.exists());
    }
}
