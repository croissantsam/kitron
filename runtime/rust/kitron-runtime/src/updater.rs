//! Auto-updater (§22): check → download → verify (ed25519) → install → restart.
//! Signature verification is mandatory; unverified payloads are rejected.
//!
//! MVP status: `check` (file:// + https:// feeds) and `verify` are real.
//! Download/install/restart with code-signing integration is Phase 4.

use kitron_core::UpdateFeed;

use base64_like::Base64;

pub async fn check(feed_url: &str) -> Result<Option<UpdateFeed>, String> {
    let body = if let Some(path) = feed_url.strip_prefix("file://") {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    async fn check_rejects_bad_scheme_and_json() {
        assert!(check("ftp://example.com/feed.json").await.is_err());
        let dir = std::env::temp_dir();
        let path = dir.join("kitron-test-feed-bad.json");
        tokio::fs::write(&path, "not json").await.unwrap();
        assert!(check(&format!("file://{}", path.display())).await.is_err());
        tokio::fs::remove_file(&path).await.ok();
    }
}
