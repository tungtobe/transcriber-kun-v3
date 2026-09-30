# Remote content publishing and model smoke

Story 6.5 adds a local signing bundle builder and a separate weekly/manual
Gemini smoke workflow. The publisher never uploads or deploys files. GitHub
Pages publication stays a release operation and requires the final HTTPS
origin, Privacy Policy URL, and named key custodian.

## Build and verify a signed Pages bundle

Build the Rust tool from the repository root:

~~~sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin remote-publish -- help
~~~

Keep the Ed25519 seed in a password manager or other approved secret store,
outside the repository. A temporary local key file can be materialized outside
the repository for a signing session, then securely removed. The file must
contain a 32-byte seed encoded as 64 hexadecimal characters or base64. The tool
resolves symlinks and rejects key paths inside the repository. Never pass the
seed on a command line or print it in a terminal transcript.

An ads source JSON contains the app's adsEnabled, optional reportUrl, and
creatives fields. Each creative must include its ID, supported locale(s), valid
schedule, positive weight, title, HTTPS click URL, and an image slot. Supply one
local image for every creative as --image <creative-id>=<path>. The builder
checks the client creative rules, HTTPS URLs, PNG/JPEG/WebP signature and
decode, 100 KiB byte limit, and the supported 300×100 or 320×50 creative
dimensions. It hashes the image bytes, writes the asset under
images/<sha256>.<extension>, and signs that URL, digest, and MIME type into
ads.json.

A recommended-settings source is the payload object described in
[recommended-settings-format.md](recommended-settings-format.md). Its schema,
model names, chunk range, template IDs/locales, text lengths, and transcript
placeholder are checked with the same validator used by the app.

Do not create a release bundle until the project owner supplies all three
release values. Use the exact Pages origin (an HTTPS origin without a path),
the final HTTPS Privacy Policy URL, and the key custodian name/team:

~~~sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin remote-publish -- build \
  --kind ads \
  --input /path/to/ads-payload.json \
  --key /path/outside/repository/remote-signing-key \
  --origin "https://<final-pages-origin>" \
  --privacy-policy-url "https://<final-privacy-policy-url>" \
  --key-custodian "<custodian name or team>" \
  --out /path/to/new-remote-bundle \
  --image summer-offer=/path/to/summer-offer.png
~~~

The output directory must not exist. The command validates all inputs before
writing and finalizes the directory with one rename; failed validation leaves
no publishable bundle. For a settings-only bundle, set --kind
recommended-settings and omit --image. The CLI emits only the app's non-secret
compile-time configuration values and the public key. It never prints the
private seed.

Verify the built bundle before upload. For an ads bundle, pass each matching
creative image again:

~~~sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin remote-publish -- verify \
  --kind ads \
  --input /path/to/new-remote-bundle/ads.json \
  --public-key-base64 "$TRANS_KUN_REMOTE_ED25519_PUBLIC_KEY_B64" \
  --origin "https://<final-pages-origin>" \
  --image summer-offer=/path/to/summer-offer.png
~~~

The verifier uses the app's envelope verifier, revalidates the schema and
images, and checks each image URL against the Pages origin and digest-derived
path. Change an envelope or image after building and verification must fail.

## GitHub Pages release procedure

1. Resolve the hosting/privacy/custody open question with the project owner:
   final Pages origin, final policy URL, and named key custodian are required.
2. Confirm the policy page is live over HTTPS and contains the policy used by
   this app. Confirm the Pages origin is the origin from which both JSON
   documents and images/ will be served.
3. Build each required document bundle with remote-publish, using the same
   origin and signing key. Keep ads.json, recommended-settings.json, and
   images/ at the Pages root. Run the CLI verifier against each envelope before
   uploading.
4. Upload the verified bundle through the repository's configured GitHub Pages
   publishing mechanism. This project deliberately has no automatic Pages
   deploy workflow while the final origin and custodian are pending.
5. Fetch the deployed ads.json, recommended-settings.json, every referenced
   image, and Privacy Policy URL over HTTPS. Re-run signature verification on
   the fetched envelopes and compare image bytes to the signed digests.
6. Compile the app with the three values emitted by remote-publish build:

   ~~~sh
   TRANS_KUN_REMOTE_ORIGIN="https://<final-pages-origin>" \
   TRANS_KUN_REMOTE_ED25519_PUBLIC_KEY_B64="<public-key-from-tool>" \
   TRANS_KUN_PRIVACY_POLICY_URL="https://<final-privacy-policy-url>" \
   cargo build --locked --manifest-path src-tauri/Cargo.toml
   ~~~

   The key is public configuration embedded in the client binary. The signing
   seed is never a build variable and never belongs in GitHub Actions build
   secrets. If the final Privacy Policy host changes, verify the Tauri opener
   URL allow-list also permits that exact host before release.

## Signing-key custody, recovery, and rotation

- Assign one named custodian and one recovery custodian before the first
  publication. Store the private seed in an access-controlled secret manager
  with an offline recovery copy under the organization's normal custody policy.
- Use the signing tool only on a trusted workstation. The seed file must be
  outside the repository; restrict its filesystem permissions and remove any
  temporary materialization after the signing session.
- Back up the encrypted seed and record its key identifier, creation date,
  custodians, and recovery procedure in the private custody register. Do not
  place that register, the seed, or API keys in Pages or the source repository.
- If the key is lost or exposed, stop publishing with it. Generate a new
  Ed25519 key, update the embedded public key, build and verify a new client
  binary, and distribute that client before relying on content signed by the
  new key. The old binary cannot trust a replacement key. Re-sign and verify
  all current remote documents after the client rollout.
- For planned rotation, build the new client and signed Pages bundle together,
  stage the bundle, release the client containing its public key, then switch
  Pages content. Keep the previous private key sealed until rollout recovery
  is no longer needed; never use it to sign new content after the cutover.

## Weekly/manual model smoke

The Gemini model smoke workflow runs every Monday at 03:17 UTC and supports
workflow_dispatch. Add a dedicated GEMINI_SMOKE_API_KEY repository Actions
secret. This key is separate from developer keys and signing keys; the job
passes it only as an environment variable to the smoke binary. The binary does
not print it or print remote response bodies.
The product owner must subscribe to failed workflow notifications for this
repository before enabling the schedule; the job exits nonzero and emits
GitHub Actions errors for both model checks when either fails.

The smoke uses the app's production Live connector and waits for the
setupComplete-derived connected event with the compiled default Live model.
It then decodes src-tauri/tests/fixtures/media/sample.wav, builds a real FLAC
chunk through the app's media pipeline, sends it through the production
transcription adapter with the compiled default transcription model, and
requires a valid parsed transcription response. Both checks run even when the
other fails.

On failure, the job reports which model check failed and directs the owner to
check the test key and update the model defaults and
[recommended-settings format](recommended-settings-format.md) if a model was
retired or its setup changed. Update
src-tauri/src/core/model_defaults.rs only after verifying the supported
model/API configuration; run the workflow manually to confirm the change
before waiting for its next scheduled run. The smoke key is a normal API key,
so quota or access changes can also fail the check; CI output intentionally
does not include the raw service error or key.
