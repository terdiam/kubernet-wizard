//! Persisted app preferences + shared UI state (GPUI global).

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use gpui_kit::Global;
use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default)]
    pub lang: Lang,
    #[serde(default)]
    pub advanced: bool,
    #[serde(default)]
    pub welcomed: bool,
    #[serde(default)]
    pub update: UpdatePrefs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePrefs {
    #[serde(default = "yes")]
    pub auto_check: bool,
    #[serde(default)]
    pub auto_download: bool,
    #[serde(default)]
    pub channel: kw_update::Channel,
    /// Unix seconds of the last automatic check.
    #[serde(default)]
    pub last_check: u64,
}

const fn yes() -> bool {
    true
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self { auto_check: true, auto_download: false, channel: kw_update::Channel::Stable, last_check: 0 }
    }
}

/// Global UI state read by every view during render.
#[derive(Debug, Clone, Default)]
pub struct Ui {
    pub prefs: Prefs,
    /// Active tour step index.
    pub tour: Option<usize>,
    /// Shown once after startup (e.g. "updated to vX").
    pub startup_notice: Option<gpui_kit::SharedString>,
}

impl Global for Ui {}

impl Ui {
    /// Tour target id currently highlighted, if any.
    #[inline]
    pub fn tour_target(&self) -> Option<&'static str> {
        self.tour.and_then(|i| crate::onboarding::MAIN_TOUR.get(i)).map(|s| s.target)
    }
}

fn dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "kubernet-wizard", "kubernet-wizard")
}

fn prefs_path() -> Option<PathBuf> {
    dirs().map(|d| d.config_dir().join("prefs.yaml"))
}

/// Update state (rollback marker, backups bookkeeping).
pub fn state_dir() -> PathBuf {
    dirs().map_or_else(|| PathBuf::from(".kw-state"), |d| d.data_dir().join("state"))
}

/// Downloaded update artifacts.
pub fn updates_dir() -> PathBuf {
    dirs().map_or_else(|| PathBuf::from(".kw-updates"), |d| d.cache_dir().join("updates"))
}

/// Verified catalog cache.
pub fn catalog_dir() -> PathBuf {
    dirs().map_or_else(|| PathBuf::from(".kw-catalog"), |d| d.data_dir().to_path_buf())
}

/// Default parent folder for generated projects.
pub fn projects_dir() -> PathBuf {
    dirs().map_or_else(|| PathBuf::from("kubernet-wizard-projects"), |d| d.data_dir().join("projects"))
}

pub fn load() -> Prefs {
    prefs_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_yaml::from_str(&t).ok())
        .unwrap_or_default()
}

/// Best-effort save; preferences are not critical.
pub fn save(prefs: &Prefs) {
    let Some(path) = prefs_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_yaml::to_string(prefs) {
        let _ = fs::write(path, text);
    }
}
