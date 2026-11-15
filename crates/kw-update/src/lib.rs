//! Update check, signed download, install and rollback.
//!
//! Trust model: every artifact is verified with SHA-256 **and** a minisign
//! signature made by the release key. The public key is embedded at build
//! time via `KW_UPDATE_PUBKEY`; builds without it can check for updates but
//! refuse to install (fail closed).

mod apply;
mod catalog;
mod download;

use std::fmt::Write as _;
use std::time::Duration;

use semver::Version;
use serde::Deserialize;

pub use apply::{Install, StartupOutcome, apply, detect_install, mark_healthy, on_startup, relaunch};
pub use catalog::{Catalog, fetch_catalog, load_cached_catalog};
pub use download::{Progress, download, download_with_key};

/// Stable feed: assets of the latest non-prerelease GitHub release.
pub const RELEASES: &str = "https://github.com/terdiam/kubernet-wizard/releases/latest/download";


/// Base64 minisign public key, embedded at build time.
pub const PUBLIC_KEY: Option<&str> = option_env!("KW_UPDATE_PUBKEY");

/// Minimum time between automatic checks.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("network: {0}")]
    Http(String),
    #[error("invalid manifest: {0}")]
    Manifest(String),
    #[error("no build published for {0}")]
    NoPlatform(String),
    #[error("this build has no update signing key; download the release manually")]
    NoTrustedKey,
    #[error("checksum mismatch (expected {expected}, got {actual})")]
    Checksum { expected: String, actual: String },
    #[error("signature verification failed: {0}")]
    Signature(String),
    #[error("{0}")]
    Unsupported(&'static str),
    #[error("io {path}: {source}")]
    Io { path: String, source: std::io::Error },
}

impl UpdateError {
    pub(crate) fn io(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> Self + '_ {
        move |source| Self::Io { path: path.display().to_string(), source }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// Manifest URL for this channel.
    #[inline]
    pub const fn manifest_url(self) -> &'static str {
        match self {
            Self::Stable => "https://github.com/terdiam/kubernet-wizard/releases/latest/download/latest.json",
            Self::Beta => "https://github.com/terdiam/kubernet-wizard/releases/download/beta/latest.json",
        }
    }

    #[inline]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stable => "Stable",
            Self::Beta => "Beta",
        }
    }
}

/// How the artifact must be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// `.tar.gz` containing `kubernet-wizard.app` (macOS).
    AppTarGz,
    /// Raw executable replacing the current one (Linux binary / AppImage, Windows exe).
    Binary,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    /// Full content of the `.minisig` file.
    pub signature: String,
    pub kind: AssetKind,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: String,
    pub platforms: std::collections::BTreeMap<String, Asset>,
}

/// An update newer than the running version.
#[derive(Debug, Clone)]
pub struct Update {
    pub version: Version,
    pub notes: String,
    pub pub_date: String,
    pub asset: Asset,
}

/// `<os>-<arch>`, e.g. `macos-aarch64`.
pub fn platform_key() -> String {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let mut key = String::with_capacity(os.len() + arch.len() + 1);
    let _ = write!(key, "{os}-{arch}");
    key
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).build().into()
}

pub(crate) fn user_agent() -> String {
    format!("kubernet-wizard/{}", env!("CARGO_PKG_VERSION"))
}

pub(crate) fn get_text(url: &str) -> Result<String, UpdateError> {
    agent()
        .get(url)
        .header("User-Agent", &user_agent())
        .call()
        .map_err(|e| UpdateError::Http(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| UpdateError::Http(e.to_string()))
}

/// Picks the asset for this platform when `manifest` is newer than `current`.
pub fn evaluate(manifest: Manifest, current: &Version, platform: &str) -> Result<Option<Update>, UpdateError> {
    let version = Version::parse(manifest.version.trim_start_matches('v'))
        .map_err(|e| UpdateError::Manifest(format!("version: {e}")))?;
    if version <= *current {
        return Ok(None);
    }
    let Manifest { notes, pub_date, mut platforms, .. } = manifest;
    let asset = platforms.remove(platform).ok_or_else(|| UpdateError::NoPlatform(platform.to_owned()))?;
    Ok(Some(Update { version, notes, pub_date, asset }))
}

/// Fetches a manifest (normally [`Channel::manifest_url`]).
pub fn check(manifest_url: &str, current: &Version) -> Result<Option<Update>, UpdateError> {
    let text = get_text(manifest_url)?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    evaluate(manifest, current, &platform_key())
}

/// Running app version.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

pub(crate) fn public_key() -> Result<minisign_verify::PublicKey, UpdateError> {
    public_key_from(PUBLIC_KEY)
}

pub fn public_key_from(key: Option<&str>) -> Result<minisign_verify::PublicKey, UpdateError> {
    let key = key.map(str::trim).filter(|k| !k.is_empty()).ok_or(UpdateError::NoTrustedKey)?;
    // Accept either the bare base64 line or a full `.pub` file.
    let line = key.lines().map(str::trim).rfind(|l| !l.is_empty() && !l.starts_with("untrusted comment")).unwrap_or(key);
    minisign_verify::PublicKey::from_base64(line).map_err(|e| UpdateError::Signature(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> Manifest {
        serde_json::from_str(&format!(
            r#"{{"version":"{version}","notes":"n","platforms":{{"linux-x86_64":{{"url":"u","sha256":"s","signature":"sig","kind":"binary"}}}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn newer_version_selected() {
        let cur = Version::new(0, 1, 0);
        let up = evaluate(manifest("v0.2.0"), &cur, "linux-x86_64").unwrap().unwrap();
        assert_eq!(up.version, Version::new(0, 2, 0));
        assert_eq!(up.asset.kind, AssetKind::Binary);
    }

    #[test]
    fn same_or_older_is_none() {
        let cur = Version::new(0, 2, 0);
        assert!(evaluate(manifest("0.2.0"), &cur, "linux-x86_64").unwrap().is_none());
        assert!(evaluate(manifest("0.1.9"), &cur, "linux-x86_64").unwrap().is_none());
    }

    #[test]
    fn prerelease_is_older_than_release() {
        let cur = Version::new(0, 2, 0);
        assert!(evaluate(manifest("0.2.0-beta.1"), &cur, "linux-x86_64").unwrap().is_none());
    }

    #[test]
    fn missing_platform_errors() {
        let cur = Version::new(0, 1, 0);
        assert!(matches!(evaluate(manifest("0.2.0"), &cur, "windows-x86_64"), Err(UpdateError::NoPlatform(_))));
    }

    #[test]
    fn missing_key_fails_closed() {
        assert!(matches!(public_key_from(None), Err(UpdateError::NoTrustedKey)));
        assert!(matches!(public_key_from(Some("  ")), Err(UpdateError::NoTrustedKey)));
    }
}
