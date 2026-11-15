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
}

/// Global UI state read by every view during render.
#[derive(Debug, Clone, Default)]
pub struct Ui {
    pub prefs: Prefs,
    /// Active tour step index.
    pub tour: Option<usize>,
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
