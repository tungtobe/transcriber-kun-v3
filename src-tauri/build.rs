fn main() {
    println!("cargo:rerun-if-env-changed=TRANS_KUN_REMOTE_ORIGIN");
    println!("cargo:rerun-if-env-changed=TRANS_KUN_REMOTE_ED25519_PUBLIC_KEY_B64");
    println!("cargo:rerun-if-env-changed=TRANS_KUN_PRIVACY_POLICY_URL");
    tauri_build::build()
}
