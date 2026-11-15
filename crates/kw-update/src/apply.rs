//! Installs a verified artifact and handles rollback.
//!
//! Flow: `apply` backs up the current install, swaps in the new one and
//! writes a marker. The new version calls `on_startup` early: after
//! [`MAX_ATTEMPTS`] starts without `mark_healthy`, the backup is restored.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::{AssetKind, UpdateError};

const MARKER: &str = "pending-update.json";
const MAX_ATTEMPTS: u8 = 3;

/// How the running app is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Install {
    /// macOS `.app` bundle at this path.
    Bundle(PathBuf),
    /// Stand-alone executable (Windows exe, Linux binary or AppImage file).
    Binary(PathBuf),
    /// Installed by a package manager (deb/rpm): never self-update.
    PackageManaged,
}

impl Install {
    #[inline]
    pub const fn supports(&self, kind: AssetKind) -> bool {
        matches!((self, kind), (Self::Bundle(_), AssetKind::AppTarGz) | (Self::Binary(_), AssetKind::Binary))
    }

    fn target(&self) -> Option<&Path> {
        match self {
            Self::Bundle(p) | Self::Binary(p) => Some(p),
            Self::PackageManaged => None,
        }
    }
}

pub fn detect_install() -> Install {
    let Ok(exe) = std::env::current_exe() else { return Install::PackageManaged };
    let exe = exe.canonicalize().unwrap_or(exe);
    if cfg!(target_os = "macos")
        && let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"))
    {
        return Install::Bundle(bundle.to_path_buf());
    }
    if cfg!(target_os = "linux") {
        if let Some(appimage) = std::env::var_os("APPIMAGE") {
            return Install::Binary(PathBuf::from(appimage));
        }
        if exe.starts_with("/usr") {
            return Install::PackageManaged;
        }
    }
    Install::Binary(exe)
}

#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    version: String,
    install: Install,
    backup: PathBuf,
    attempts: u8,
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map_or_else(std::ffi::OsString::new, std::ffi::OsStr::to_os_string);
    name.push(suffix);
    path.with_file_name(name)
}

fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

fn write_marker(state_dir: &Path, m: &Marker) -> Result<(), UpdateError> {
    fs::create_dir_all(state_dir).map_err(UpdateError::io(state_dir))?;
    let path = state_dir.join(MARKER);
    let text = serde_json::to_string(m).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    fs::write(&path, text).map_err(UpdateError::io(&path))
}

fn read_marker(state_dir: &Path) -> Option<Marker> {
    serde_json::from_str(&fs::read_to_string(state_dir.join(MARKER)).ok()?).ok()
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<(), UpdateError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(UpdateError::io(path))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<(), UpdateError> {
    Ok(())
}

/// Extracts the `.app` from a tar.gz next to the current bundle.
fn stage_bundle(archive: &Path, current: &Path) -> Result<PathBuf, UpdateError> {
    let staging = sibling(current, ".kw-staging");
    remove_any(&staging);
    fs::create_dir_all(&staging).map_err(UpdateError::io(&staging))?;
    let file = fs::File::open(archive).map_err(UpdateError::io(archive))?;
    tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(&staging).map_err(UpdateError::io(&staging))?;
    let app = fs::read_dir(&staging)
        .map_err(UpdateError::io(&staging))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or(UpdateError::Unsupported("archive does not contain a .app bundle"))?;
    Ok(app)
}

/// Installs `artifact` (already verified). Returns the path to relaunch.
pub fn apply(artifact: &Path, kind: AssetKind, install: &Install, version: &str, state_dir: &Path) -> Result<PathBuf, UpdateError> {
    if !install.supports(kind) {
        return Err(match install {
            Install::PackageManaged => UpdateError::Unsupported("installed by a package manager; update it with apt/dnf"),
            _ => UpdateError::Unsupported("this installation type cannot use the published artifact; download it manually"),
        });
    }
    let target = install.target().ok_or(UpdateError::Unsupported("no install target"))?;
    let backup = sibling(target, ".kw-backup");
    remove_any(&backup);

    match install {
        Install::Bundle(current) => {
            let staged = stage_bundle(artifact, current)?;
            fs::rename(current, &backup).map_err(UpdateError::io(current))?;
            if let Err(e) = fs::rename(&staged, current) {
                let _ = fs::rename(&backup, current);
                return Err(UpdateError::io(current)(e));
            }
            remove_any(&sibling(current, ".kw-staging"));
        }
        Install::Binary(current) => {
            fs::copy(current, &backup).map_err(UpdateError::io(&backup))?;
            if cfg!(windows) {
                self_replace::self_replace(artifact).map_err(UpdateError::io(current))?;
            } else {
                let staged = sibling(current, ".kw-new");
                fs::copy(artifact, &staged).map_err(UpdateError::io(&staged))?;
                make_executable(&staged)?;
                fs::rename(&staged, current).map_err(UpdateError::io(current))?;
            }
        }
        Install::PackageManaged => return Err(UpdateError::Unsupported("package-managed install")),
    }
    let _ = fs::remove_file(artifact);
    write_marker(state_dir, &Marker { version: version.to_owned(), install: install.clone(), backup, attempts: 0 })?;
    Ok(target.to_path_buf())
}

fn restore(m: &Marker) -> Result<(), UpdateError> {
    match &m.install {
        Install::Bundle(current) => {
            remove_any(current);
            fs::rename(&m.backup, current).map_err(UpdateError::io(current))
        }
        Install::Binary(current) if cfg!(windows) => {
            self_replace::self_replace(&m.backup).map_err(UpdateError::io(current))?;
            let _ = fs::remove_file(&m.backup);
            Ok(())
        }
        Install::Binary(current) => fs::rename(&m.backup, current).map_err(UpdateError::io(current)),
        Install::PackageManaged => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupOutcome {
    /// No update in flight.
    Normal,
    /// First starts after an update; call [`mark_healthy`] once the UI is up.
    JustUpdated { version: String },
    /// Previous version restored after repeated failed starts; relaunch it.
    RolledBack { relaunch: PathBuf },
}

/// Call as early as possible on startup.
pub fn on_startup(state_dir: &Path, current_version: &str) -> StartupOutcome {
    let Some(mut m) = read_marker(state_dir) else { return StartupOutcome::Normal };
    if m.version != current_version {
        // Running something else (manual reinstall or already restored): drop state.
        remove_any(&m.backup);
        let _ = fs::remove_file(state_dir.join(MARKER));
        return StartupOutcome::Normal;
    }
    m.attempts = m.attempts.saturating_add(1);
    if m.attempts > MAX_ATTEMPTS {
        let _ = fs::remove_file(state_dir.join(MARKER));
        return match (restore(&m), m.install.target()) {
            (Ok(()), Some(t)) => StartupOutcome::RolledBack { relaunch: t.to_path_buf() },
            _ => StartupOutcome::Normal,
        };
    }
    let _ = write_marker(state_dir, &m);
    StartupOutcome::JustUpdated { version: m.version }
}

/// New version started fine: delete backup and marker.
pub fn mark_healthy(state_dir: &Path) {
    if let Some(m) = read_marker(state_dir) {
        remove_any(&m.backup);
    }
    let _ = fs::remove_file(state_dir.join(MARKER));
}

/// Starts the given install (new process). Caller then quits.
pub fn relaunch(target: &Path) -> Result<(), UpdateError> {
    let spawned = if cfg!(target_os = "macos") && target.extension().is_some_and(|e| e == "app") {
        Command::new("open").arg("-n").arg(target).spawn()
    } else {
        Command::new(target).spawn()
    };
    spawned.map(drop).map_err(UpdateError::io(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join("kw-apply-test").join(name);
        remove_any(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[cfg(unix)]
    #[test]
    fn binary_apply_then_healthy() {
        let d = tmp("healthy");
        let state = d.join("state");
        let current = d.join("app-bin");
        let new = d.join("download.bin");
        fs::write(&current, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        let install = Install::Binary(current.clone());

        apply(&new, AssetKind::Binary, &install, "0.2.0", &state).unwrap();
        assert_eq!(fs::read(&current).unwrap(), b"new");
        assert!(sibling(&current, ".kw-backup").exists());

        assert_eq!(on_startup(&state, "0.2.0"), StartupOutcome::JustUpdated { version: "0.2.0".into() });
        mark_healthy(&state);
        assert!(!sibling(&current, ".kw-backup").exists());
        assert_eq!(on_startup(&state, "0.2.0"), StartupOutcome::Normal);
    }

    #[cfg(unix)]
    #[test]
    fn binary_rollback_after_failed_starts() {
        let d = tmp("rollback");
        let state = d.join("state");
        let current = d.join("app-bin");
        let new = d.join("download.bin");
        fs::write(&current, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        apply(&new, AssetKind::Binary, &Install::Binary(current.clone()), "0.2.0", &state).unwrap();

        for _ in 0..MAX_ATTEMPTS {
            assert!(matches!(on_startup(&state, "0.2.0"), StartupOutcome::JustUpdated { .. }));
        }
        assert_eq!(on_startup(&state, "0.2.0"), StartupOutcome::RolledBack { relaunch: current.clone() });
        assert_eq!(fs::read(&current).unwrap(), b"old");
    }

    #[test]
    fn kind_mismatch_refused() {
        let d = tmp("mismatch");
        let r = apply(&d.join("x"), AssetKind::AppTarGz, &Install::Binary(d.join("bin")), "0.2.0", &d);
        assert!(matches!(r, Err(UpdateError::Unsupported(_))));
        let r = apply(&d.join("x"), AssetKind::Binary, &Install::PackageManaged, "0.2.0", &d);
        assert!(matches!(r, Err(UpdateError::Unsupported(_))));
    }

    #[cfg(unix)]
    #[test]
    fn bundle_apply_from_tar_gz() {
        let d = tmp("bundle");
        let state = d.join("state");
        let current = d.join("kubernet-wizard.app");
        fs::create_dir_all(current.join("Contents")).unwrap();
        fs::write(current.join("Contents/v"), b"old").unwrap();

        // Build archive containing kubernet-wizard.app/Contents/v = new
        let src = d.join("src/kubernet-wizard.app/Contents");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("v"), b"new").unwrap();
        let archive = d.join("update.tar.gz");
        let enc = flate2::write::GzEncoder::new(fs::File::create(&archive).unwrap(), flate2::Compression::fast());
        let mut tb = tar::Builder::new(enc);
        tb.append_dir_all("kubernet-wizard.app", d.join("src/kubernet-wizard.app")).unwrap();
        tb.into_inner().unwrap().finish().unwrap();

        apply(&archive, AssetKind::AppTarGz, &Install::Bundle(current.clone()), "0.2.0", &state).unwrap();
        assert_eq!(fs::read(current.join("Contents/v")).unwrap(), b"new");
        assert_eq!(fs::read(sibling(&current, ".kw-backup").join("Contents/v")).unwrap(), b"old");
    }
}
