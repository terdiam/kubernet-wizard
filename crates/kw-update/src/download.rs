//! Streaming download with SHA-256 + minisign verification in one pass.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use minisign_verify::Signature;
use sha2::{Digest, Sha256};

use crate::{Asset, UpdateError, agent, public_key, user_agent};

/// Lock-free progress shared with the UI.
#[derive(Debug, Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

const CHUNK: usize = 64 * 1024;

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0xf)]));
    }
    out
}

/// Verifies `reader` against the asset while copying it to `out`.
/// The file is deleted on any failure.
pub(crate) fn verify_copy(
    mut reader: impl Read,
    asset: &Asset,
    key: &minisign_verify::PublicKey,
    out: &Path,
    progress: &Progress,
) -> Result<(), UpdateError> {
    let sig = Signature::decode(&asset.signature).map_err(|e| UpdateError::Signature(e.to_string()))?;
    let mut verifier = key.verify_stream(&sig).map_err(|e| UpdateError::Signature(e.to_string()))?;
    let mut sha = Sha256::new();
    let file = File::create(out).map_err(UpdateError::io(out))?;
    let mut writer = BufWriter::new(file);
    let mut buf = vec![0u8; CHUNK];

    let result = (|| {
        loop {
            if progress.cancel.load(Ordering::Relaxed) {
                return Err(UpdateError::Unsupported("download cancelled"));
            }
            let n = reader.read(&mut buf).map_err(|e| UpdateError::Http(e.to_string()))?;
            if n == 0 {
                break;
            }
            let chunk = &buf[..n];
            sha.update(chunk);
            verifier.update(chunk);
            writer.write_all(chunk).map_err(UpdateError::io(out))?;
            progress.done.fetch_add(n as u64, Ordering::Relaxed);
        }
        writer.flush().map_err(UpdateError::io(out))?;
        let actual = hex(&sha.finalize());
        if !actual.eq_ignore_ascii_case(asset.sha256.trim()) {
            return Err(UpdateError::Checksum { expected: asset.sha256.clone(), actual });
        }
        verifier.finalize().map_err(|e| UpdateError::Signature(e.to_string()))
    })();

    if result.is_err() {
        drop(writer);
        let _ = fs::remove_file(out);
    }
    result
}

/// Downloads and verifies the asset into `dir` with the embedded release key.
pub fn download(asset: &Asset, dir: &Path, progress: &Progress) -> Result<PathBuf, UpdateError> {
    download_with_key(asset, dir, progress, &public_key()?)
}

/// Same as [`download`] with an explicit key (tests, custom builds).
pub fn download_with_key(
    asset: &Asset,
    dir: &Path,
    progress: &Progress,
    key: &minisign_verify::PublicKey,
) -> Result<PathBuf, UpdateError> {
    fs::create_dir_all(dir).map_err(UpdateError::io(dir))?;
    let name = asset.url.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("kubernet-wizard-update");
    let out = dir.join(name);

    let resp = agent()
        .get(&asset.url)
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|e| UpdateError::Http(e.to_string()))?;
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .or(asset.size)
        .unwrap_or(0);
    progress.total.store(total, Ordering::Relaxed);
    progress.done.store(0, Ordering::Relaxed);

    let reader = resp.into_body().into_reader();
    verify_copy(reader, asset, key, &out, progress)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetKind, public_key_from};

    // Test-only key pair; not the release key.
    const PUB: &str = include_str!("../tests/fixtures/test-key.pub");
    const DATA: &[u8] = include_bytes!("../tests/fixtures/hello.txt");
    const SIG: &str = include_str!("../tests/fixtures/hello.txt.minisig");
    const SHA: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    fn asset(sha: &str) -> Asset {
        Asset { url: "x".into(), sha256: sha.into(), signature: SIG.into(), kind: AssetKind::Binary, size: None }
    }

    fn run(data: &[u8], sha: &str, name: &str) -> (Result<(), UpdateError>, PathBuf) {
        let key = public_key_from(Some(PUB)).unwrap();
        let dir = std::env::temp_dir().join("kw-update-test");
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join(name);
        (verify_copy(data, &asset(sha), &key, &out, &Progress::default()), out)
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }

    #[test]
    fn valid_artifact_passes() {
        let (r, out) = run(DATA, SHA, "ok.bin");
        assert!(r.is_ok(), "{r:?}");
        assert_eq!(fs::read(&out).unwrap(), DATA);
    }

    #[test]
    fn tampered_data_rejected_and_removed() {
        // Correct checksum of the tampered bytes, but signature is for the original.
        let tampered = b"hello worle";
        let sha = hex(&Sha256::digest(tampered));
        let (r, out) = run(tampered, &sha, "tampered.bin");
        assert!(matches!(r, Err(UpdateError::Signature(_))), "{r:?}");
        assert!(!out.exists());
    }

    #[test]
    fn checksum_mismatch_rejected() {
        let (r, out) = run(DATA, "00", "sha.bin");
        assert!(matches!(r, Err(UpdateError::Checksum { .. })), "{r:?}");
        assert!(!out.exists());
    }
}
