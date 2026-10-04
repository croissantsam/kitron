//! Auto-updater (§22): check → download → verify (ed25519) → install → restart.
//! Signature verification is mandatory; unverified payloads are rejected.
//!
//! MVP status: `check` (file:// + https:// feeds) and `verify` are real.
//! Download/install/restart with code-signing integration is Phase 4.

use kitron_core::UpdateFeed;

use base64_like::Base64;

pub async fn check(feed_url: &str) -> Result<Option<UpdateFeed>, String> {    let body = if let Some(path) = feed_url.strip_prefix("file://") {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("UPDATE_CHECK_FAILED: cannot read feed: {e}"))?
    } else if feed_url.starts_with("https://") || feed_url.starts_with("http://") {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"))?;
        let res = client
            .get(feed_url)
            .send()
            .await
            .map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("UPDATE_CHECK_FAILED: feed returned {}", res.status()));
        }
        res.text().await.map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"))?
    } else {
        return Err("UPDATE_CHECK_FAILED: feed URL must be file://, http:// or https://".into());
    };
    // Feed may be a single object or `{ "latest": {...} }`.
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("UPDATE_CHECK_FAILED: bad feed JSON: {e}"))?;
    let entry = v.get("latest").unwrap_or(&v);
    let feed: UpdateFeed = serde_json::from_value(entry.clone())
        .map_err(|e| format!("UPDATE_CHECK_FAILED: bad feed entry: {e}"))?;
    if feed.version.trim().is_empty() {
        return Err("UPDATE_CHECK_FAILED: feed entry has no version".into());
    }
    Ok(Some(feed))
}

pub fn verify_signature(payload: &[u8], signature_b64: &str, pubkey_hex: &str) -> Result<(), String> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    use sha2::{Digest, Sha256};

    let _digest = Sha256::digest(payload);
    let sig_bytes = Base64::decode(signature_b64).map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())?;
    let pk_bytes = hex_decode(pubkey_hex).map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())?;
    if sig_bytes.len() != 64 || pk_bytes.len() != 32 {
        return Err("UPDATE_SIGNATURE_INVALID".into());
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(payload, &sig).map_err(|_| "UPDATE_SIGNATURE_INVALID".to_string())
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

mod base64_like {
    pub struct Base64;
    impl Base64 {
        pub fn decode(s: &str) -> Result<Vec<u8>, ()> {
            // minimal standard-base64 decode (no external crate to keep MSRV low)
            const T: &[u8; 128] = &{
                let mut t = [255u8; 128];
                let mut i = 0u8;
                while i < 26 {
                    t[(b'A' + i) as usize] = i;
                    t[(b'a' + i) as usize] = 26 + i;
                    i += 1;
                }
                let mut j = 0u8;
                while j < 10 {
                    t[(b'0' + j) as usize] = 52 + j;
                    j += 1;
                }
                t[b'+' as usize] = 62;
                t[b'/' as usize] = 63;
                t
            };
            let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
            let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
            for c in bytes.chunks(4) {
                let mut n = 0u32;
                let mut bits = 0;
                for &b in c {
                    if b >= 128 || T[b as usize] == 255 {
                        return Err(());
                    }
                    n = (n << 6) | T[b as usize] as u32;
                    bits += 6;
                }
                while bits >= 8 {
                    bits -= 8;
                    out.push((n >> bits) as u8 & 0xFF);
                }
            }
            Ok(out)
        }
    }
}

/// Download an update payload to the per-app staging dir.
/// Returns the staged file path. Verification happens at `install` time —
/// never execute staged bytes before `verify_signature` passes.
pub async fn download(url: &str, version: &str) -> Result<std::path::PathBuf, String> {
    let bytes = if let Some(path) = url.strip_prefix("file://") {
        tokio::fs::read(path)
            .await
            .map_err(|e| format!("UPDATE_DOWNLOAD_FAILED: cannot read {url}: {e}"))?
    } else if url.starts_with("https://") || url.starts_with("http://") {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| format!("UPDATE_DOWNLOAD_FAILED: {e}"))?;
        let res = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("UPDATE_DOWNLOAD_FAILED: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("UPDATE_DOWNLOAD_FAILED: server returned {}", res.status()));
        }
        res.bytes()
            .await
            .map_err(|e| format!("UPDATE_DOWNLOAD_FAILED: {e}"))?
            .to_vec()
    } else {
        return Err("UPDATE_DOWNLOAD_FAILED: URL must be file://, http:// or https://".into());
    };
    let dir = staging_dir()?;
    let safe_version: String =
        version.chars().map(|c| if c.is_alphanumeric() || c == '.' { c } else { '_' }).collect();
    let dest = dir.join(format!("kitron-update-{safe_version}.bin"));
    tokio::fs::write(&dest, &bytes)
        .await
        .map_err(|e| format!("UPDATE_DOWNLOAD_FAILED: cannot stage update: {e}"))?;
    Ok(dest)
}

fn staging_dir() -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("kitron-updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("UPDATE_DOWNLOAD_FAILED: {e}"))?;
    Ok(dir)
}

/// Verify + swap the staged payload over the current executable, keeping a
/// `.bak` backup. Returns the backup path. Unix only: on Windows the running
/// image cannot be replaced, so `install` verifies + stages and leaves the
/// swap to the installer (Phase 4).
pub async fn install(
    staged: &std::path::Path,
    signature_b64: &str,
    pubkey_hex: &str,
) -> Result<std::path::PathBuf, String> {
    install_to(staged, signature_b64, pubkey_hex, None).await
}

/// `target_override` exists for tests: production always swaps
/// `current_exe`. The backup/rollback semantics are identical.
pub async fn install_to(
    staged: &std::path::Path,
    signature_b64: &str,
    pubkey_hex: &str,
    target_override: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let bytes = tokio::fs::read(staged)
        .await
        .map_err(|e| format!("UPDATE_CHECK_FAILED: cannot read staged update: {e}"))?;
    verify_signature(&bytes, signature_b64, pubkey_hex)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let current = match target_override {
            Some(t) => t.to_path_buf(),
            None => std::env::current_exe().map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"))?,
        };
        let backup = current.with_extension("bak");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(&current, &backup)
            .map_err(|e| format!("UPDATE_CHECK_FAILED: cannot back up current binary: {e}"))?;
        if let Err(e) = std::fs::rename(staged, &current) {
            // Roll back: restore the backup so the app is never left broken.
            let _ = std::fs::rename(&backup, &current);
            return Err(format!("UPDATE_CHECK_FAILED: install failed, rolled back: {e}"));
        }
        let mut perms = std::fs::metadata(&current)
            .map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current, perms)
            .map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"))?;
        Ok(backup)
    }
    #[cfg(not(unix))]
    {
        let _ = staged;
        Err("UPDATE_CHECK_FAILED: self-swap is Unix-only; Windows updates ride the installer (Phase 4)".into())
    }
}

/// Staged-update record shared between `download` → `install` →
/// `installAndRestart`.
#[derive(Debug, Clone, Default)]
pub struct StagedUpdate {
    pub path: Option<std::path::PathBuf>,
    pub version: Option<String>,
    pub signature: Option<String>,
    pub public_key: Option<String>,
    pub installed_backup: Option<std::path::PathBuf>,
}

pub struct UpdaterState {
    staged: tokio::sync::Mutex<StagedUpdate>,
}

impl UpdaterState {
    pub fn new() -> Self {
        Self { staged: tokio::sync::Mutex::new(StagedUpdate::default()) }
    }

    pub async fn set_downloaded(&self, path: std::path::PathBuf, version: String) {
        let mut s = self.staged.lock().await;
        s.path = Some(path);
        s.version = Some(version);
    }

    pub async fn set_verified(&self, signature: String, public_key: String, backup: std::path::PathBuf) {
        let mut s = self.staged.lock().await;
        s.signature = Some(signature);
        s.public_key = Some(public_key);
        s.installed_backup = Some(backup);
    }

    pub async fn take_for_restart(&self) -> Option<StagedUpdate> {
        let s = self.staged.lock().await.clone();
        if s.installed_backup.is_some() {
            Some(s)
        } else {
            None
        }
    }

    pub async fn staged_path(&self) -> Option<std::path::PathBuf> {
        self.staged.lock().await.path.clone()
    }
}

impl Default for UpdaterState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {    use super::*;

    #[test]
    fn rejects_bad_signature() {
        assert!(verify_signature(b"hello", "AAAA", "00").is_err());
    }

    #[test]
    fn hex_decode_ok() {
        assert_eq!(hex_decode("00ff").unwrap(), vec![0, 255]);
    }

    #[tokio::test]
    async fn check_file_feed() {
        let dir = std::env::temp_dir();
        let path = dir.join("kitron-test-feed.json");
        tokio::fs::write(
            &path,
            r#"{"version":"1.2.0","notes":"hi","url":"https://example.com/app.zip"}"#,
        )
        .await
        .unwrap();
        let feed = check(&format!("file://{}", path.display())).await.unwrap().unwrap();
        assert_eq!(feed.version, "1.2.0");
        assert_eq!(feed.notes.unwrap(), "hi");
        tokio::fs::remove_file(&path).await.ok();
    }

    #[tokio::test]
    async fn download_verify_install_roundtrip() {
        use ed25519_dalek::{Signer, SigningKey};

        // Deterministic test keypair (never used outside tests).
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let pubkey_hex = hex::encode(&signing.verifying_key().to_bytes());

        // Payload served through a file:// "feed".
        let dir = std::env::temp_dir().join("kitron-update-e2e");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let payload_path = dir.join("payload.bin");
        tokio::fs::write(&payload_path, b"new-binary-bytes").await.unwrap();

        let staged = download(&format!("file://{}", payload_path.display()), "9.9.9").await.unwrap();
        assert!(staged.exists());

        // Wrong key → signature invalid, nothing swapped.
        let target = dir.join("app.bin");
        tokio::fs::write(&target, b"old-binary-bytes").await.unwrap();
        let bad_sig = b64_encode(&[0u8; 64]);
        assert!(
            install_to(&staged, &bad_sig, &pubkey_hex, Some(&target)).await.is_err()
        );
        assert_eq!(tokio::fs::read(&target).await.unwrap(), b"old-binary-bytes");

        // Correct signature → swap + backup, executable bit set.
        let sig = signing.sign(b"new-binary-bytes");
        let good_sig = b64_encode(&sig.to_bytes());
        let backup = install_to(&staged, &good_sig, &pubkey_hex, Some(&target)).await.unwrap();
        assert_eq!(tokio::fs::read(&target).await.unwrap(), b"new-binary-bytes");
        assert_eq!(tokio::fs::read(&backup).await.unwrap(), b"old-binary-bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(tokio::fs::metadata(&target).await.unwrap().permissions().mode() & 0o111 != 0);
        }
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    fn b64_encode(bytes: &[u8]) -> String {
        const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let n = ((c[0] as u32) << 16) | ((c.get(1).copied().unwrap_or(0) as u32) << 8) | (c.get(2).copied().unwrap_or(0) as u32);
            out.push(A[((n >> 18) & 63) as usize] as char);
            out.push(A[((n >> 12) & 63) as usize] as char);
            out.push(if c.len() > 1 { A[((n >> 6) & 63) as usize] as char } else { '=' });
            out.push(if c.len() > 2 { A[(n & 63) as usize] as char } else { '=' });
        }
        out
    }

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    mod hex {
        pub fn encode(bytes: &[u8; 32]) -> String {
            super::hex_encode(bytes)
        }
    }

    #[tokio::test]
    async fn check_rejects_bad_scheme_and_json() {
        assert!(check("ftp://example.com/feed.json").await.is_err());
        let dir = std::env::temp_dir();
        let path = dir.join("kitron-test-feed-bad.json");
        tokio::fs::write(&path, "not json").await.unwrap();
        assert!(check(&format!("file://{}", path.display())).await.is_err());
        tokio::fs::remove_file(&path).await.ok();
    }
}
