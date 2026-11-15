//! Signed version catalog (tested distro versions), refreshed independently
//! of app releases. Cached on disk after verification; offline uses cache.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use minisign_verify::Signature;
use serde::{Deserialize, Serialize};

use crate::{UpdateError, get_text, public_key};

const CACHE: &str = "catalog.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    /// Distro id (`k3s`, `rke2`, `kubeadm`, ...) → versions, newest first.
    pub distros: BTreeMap<String, Vec<String>>,
}

fn verify(text: &str, sig: &str) -> Result<Catalog, UpdateError> {
    let key = public_key()?;
    let sig = Signature::decode(sig).map_err(|e| UpdateError::Signature(e.to_string()))?;
    key.verify(text.as_bytes(), &sig, false).map_err(|e| UpdateError::Signature(e.to_string()))?;
    serde_json::from_str(text).map_err(|e| UpdateError::Manifest(e.to_string()))
}

/// Downloads `catalog.json` + signature from `base`, verifies, caches.
pub fn fetch_catalog(base: &str, cache_dir: &Path) -> Result<Catalog, UpdateError> {
    let base = base.trim_end_matches('/');
    let text = get_text(&format!("{base}/catalog.json"))?;
    let sig = get_text(&format!("{base}/catalog.json.minisig"))?;
    let catalog = verify(&text, &sig)?;
    fs::create_dir_all(cache_dir).map_err(UpdateError::io(cache_dir))?;
    let path = cache_dir.join(CACHE);
    fs::write(&path, &text).map_err(UpdateError::io(&path))?;
    let sig_path = cache_dir.join("catalog.json.minisig");
    fs::write(&sig_path, &sig).map_err(UpdateError::io(&sig_path))?;
    Ok(catalog)
}

/// Last verified catalog (re-verified on load so a tampered cache is ignored).
pub fn load_cached_catalog(cache_dir: &Path) -> Option<Catalog> {
    let text = fs::read_to_string(cache_dir.join(CACHE)).ok()?;
    let sig = fs::read_to_string(cache_dir.join("catalog.json.minisig")).ok()?;
    verify(&text, &sig).ok()
}
