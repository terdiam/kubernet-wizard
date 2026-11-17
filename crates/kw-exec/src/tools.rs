//! Installs prerequisite tools into an app-managed folder, without admin rights.
//!
//! - kubectl, Helm, OpenTofu: official release binaries, SHA-256 verified.
//! - Ansible: `ansible-core` in a Python venv (macOS/Linux; Windows uses WSL).
//! - Git / OpenSSH: the OS installer (xcode-select, winget, pkexec + apt/dnf).
//!
//! [`crate::host_command`] prefers binaries from this folder.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::prereq::Tool;

static DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("download {0}: {1}")]
    Http(String, String),
    #[error("{0}: {1}")]
    Io(String, io::Error),
    #[error("checksum mismatch for {0}")]
    Checksum(String),
    #[error("{0}")]
    Failed(String),
    #[error("automatic install is not available here: {0}")]
    Manual(&'static str),
}

/// Sets the managed tools folder once at startup.
pub fn set_dir(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// Uses `<app data>/tools`, shared by the desktop app and the `kw` CLI
/// (`KW_TOOLS_DIR` overrides it, e.g. for tests).
pub fn init_default() {
    if let Some(d) = std::env::var_os("KW_TOOLS_DIR") {
        set_dir(PathBuf::from(d));
    } else if let Some(d) = directories::ProjectDirs::from("dev", "kubernet-wizard", "kubernet-wizard") {
        set_dir(d.data_dir().join("tools"));
    }
}

/// Tool by display name or program, case-insensitive (`helm`, `OpenTofu`, `tofu`).
pub fn find(name: &str) -> Option<&'static Tool> {
    crate::prereq::TOOLS.iter().find(|t| t.name.eq_ignore_ascii_case(name) || t.program.eq_ignore_ascii_case(name))
}

#[inline]
fn bin_dir() -> Option<PathBuf> {
    DIR.get().map(|d| d.join("bin"))
}

const EXE: &str = if cfg!(windows) { ".exe" } else { "" };

/// Managed copy of `program`, if installed.
pub fn managed(program: &str) -> Option<PathBuf> {
    let p = bin_dir()?.join(format!("{program}{EXE}"));
    p.is_file().then_some(p)
}

/// Folders package managers install into. Apps started from Finder or a
/// desktop launcher get a minimal `PATH` without them.
fn well_known_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::with_capacity(6);
    if cfg!(target_os = "macos") {
        v.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    } else if cfg!(unix) {
        v.extend(["/usr/local/bin", "/snap/bin"].map(PathBuf::from));
    }
    if let Some(home) = std::env::var_os("HOME") {
        v.push(PathBuf::from(home).join(".local/bin"));
    }
    v
}

/// `PATH` for child processes: managed folder, then the process `PATH`
/// (`KW_PATH` replaces it, e.g. for tests), then well-known install folders.
pub fn path_env() -> Option<std::ffi::OsString> {
    let mut parts: Vec<PathBuf> = Vec::with_capacity(16);
    parts.extend(bin_dir());
    if let Some(over) = std::env::var_os("KW_PATH") {
        parts.extend(std::env::split_paths(&over));
    } else {
        if let Some(cur) = std::env::var_os("PATH") {
            parts.extend(std::env::split_paths(&cur));
        }
        for d in well_known_dirs() {
            if !parts.contains(&d) {
                parts.push(d);
            }
        }
    }
    std::env::join_paths(parts).ok()
}

/// How a tool gets installed on this OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Download,
    PythonVenv,
    /// Windows: `ansible-core` installed as root inside the default WSL distro.
    Wsl,
    System,
    /// No automatic path; the hint explains what to do.
    Manual(&'static str),
}

pub fn method(tool: &Tool) -> Method {
    match tool.program {
        "kubectl" | "helm" | "tofu" => Method::Download,
        "ansible-playbook" if cfg!(windows) => Method::Wsl,
        "ansible-playbook" => Method::PythonVenv,
        "ssh" if cfg!(target_os = "macos") => Method::Manual("OpenSSH ships with macOS"),
        "ssh" if cfg!(windows) => Method::Manual("Settings → System → Optional features → OpenSSH Client"),
        _ => Method::System,
    }
}

/// Installs `tool`, streaming progress lines to `log`.
pub fn install(tool: &Tool, mut log: impl FnMut(&str)) -> Result<(), InstallError> {
    let bin = bin_dir().ok_or(InstallError::Manual("tools folder not set"))?;
    fs::create_dir_all(&bin).map_err(io_err(&bin))?;
    match (method(tool), tool.program) {
        (Method::Download, "kubectl") => kubectl(&bin, &mut log),
        (Method::Download, "helm") => helm(&bin, &mut log),
        (Method::Download, _) => tofu(&bin, &mut log),
        (Method::PythonVenv, _) => ansible(&bin, &mut log),
        (Method::Wsl, _) => ansible_wsl(&mut log),
        (Method::System, program) => system(program, &mut log),
        (Method::Manual(hint), _) => Err(InstallError::Manual(hint)),
    }
}

// ------------------------------------------------------------ platform

fn os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn io_err(p: &Path) -> impl FnOnce(io::Error) -> InstallError + '_ {
    move |e| InstallError::Io(p.display().to_string(), e)
}

// ------------------------------------------------------------ http

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into()
}

fn get_text(url: &str) -> Result<String, InstallError> {
    agent()
        .get(url)
        .header("User-Agent", concat!("kubernet-wizard/", env!("CARGO_PKG_VERSION")))
        .call()
        .and_then(|mut r| r.body_mut().read_to_string())
        .map_err(|e| InstallError::Http(url.to_owned(), e.to_string()))
}

/// Streams `url` to `out` and returns its SHA-256 (hex).
fn download(url: &str, out: &Path, log: &mut impl FnMut(&str)) -> Result<String, InstallError> {
    log(&format!("Downloading {url}"));
    let resp = agent()
        .get(url)
        .header("User-Agent", concat!("kubernet-wizard/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| InstallError::Http(url.to_owned(), e.to_string()))?;
    let mut reader = resp.into_body().into_reader();
    let mut file = File::create(out).map_err(io_err(out))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(io_err(out))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(io_err(out))?;
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Expected hash for `name` from a checksum file (`<hash>  <name>` lines or a bare hash).
fn expected_sha(sums: &str, name: &str) -> Option<String> {
    let mut lines = sums.lines().filter(|l| !l.trim().is_empty());
    let found = sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, f) = (it.next()?, it.next()?);
        (f.trim_start_matches('*') == name).then(|| h.to_ascii_lowercase())
    });
    found.or_else(|| {
        let first = lines.next()?.split_whitespace().next()?;
        (first.len() == 64 && lines.next().is_none()).then(|| first.to_ascii_lowercase())
    })
}

fn verify(got: &str, sums: &str, name: &str) -> Result<(), InstallError> {
    match expected_sha(sums, name) {
        Some(want) if want == got => Ok(()),
        _ => Err(InstallError::Checksum(name.to_owned())),
    }
}

fn tmp_dir(bin: &Path) -> Result<PathBuf, InstallError> {
    let t = bin.with_file_name("tmp");
    fs::create_dir_all(&t).map_err(io_err(&t))?;
    Ok(t)
}

/// Moves `src` into `bin/<program>` and marks it executable.
fn place(src: &Path, bin: &Path, program: &str) -> Result<PathBuf, InstallError> {
    let dst = bin.join(format!("{program}{EXE}"));
    fs::copy(src, &dst).map_err(io_err(&dst))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).map_err(io_err(&dst))?;
    }
    Ok(dst)
}

// ------------------------------------------------------------ tools

fn kubectl(bin: &Path, log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    let v = get_text("https://dl.k8s.io/release/stable.txt")?;
    let v = v.trim();
    let url = format!("https://dl.k8s.io/release/{v}/bin/{}/{}/kubectl{EXE}", os(), arch());
    let tmp = tmp_dir(bin)?.join(format!("kubectl{EXE}"));
    let got = download(&url, &tmp, log)?;
    verify(&got, &get_text(&format!("{url}.sha256"))?, "kubectl")?;
    let dst = place(&tmp, bin, "kubectl")?;
    let _ = fs::remove_file(&tmp);
    log(&format!("Installed kubectl {v} → {}", dst.display()));
    Ok(())
}

fn helm(bin: &Path, log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    let v = get_text("https://get.helm.sh/helm-latest-version")?;
    let v = v.trim();
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let name = format!("helm-{v}-{}-{}.{ext}", os(), arch());
    let url = format!("https://get.helm.sh/{name}");
    let tmp = tmp_dir(bin)?;
    let archive = tmp.join(&name);
    let got = download(&url, &archive, log)?;
    verify(&got, &get_text(&format!("{url}.sha256sum"))?, &name)?;
    let inner = format!("{}-{}/helm{EXE}", os(), arch());
    let out = tmp.join(format!("helm{EXE}"));
    extract(&archive, &inner, &out)?;
    let dst = place(&out, bin, "helm")?;
    let _ = (fs::remove_file(&archive), fs::remove_file(&out));
    log(&format!("Installed Helm {v} → {}", dst.display()));
    Ok(())
}

fn tofu(bin: &Path, log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    let api = get_text("https://api.github.com/repos/opentofu/opentofu/releases/latest")?;
    let tag = serde_json::from_str::<serde_json::Value>(&api)
        .ok()
        .and_then(|j| j.get("tag_name")?.as_str().map(str::to_owned))
        .ok_or_else(|| InstallError::Failed("OpenTofu: latest release tag not found".into()))?;
    let ver = tag.trim_start_matches('v');
    let base = format!("https://github.com/opentofu/opentofu/releases/download/{tag}");
    let name = format!("tofu_{ver}_{}_{}.zip", os(), arch());
    let tmp = tmp_dir(bin)?;
    let archive = tmp.join(&name);
    let got = download(&format!("{base}/{name}"), &archive, log)?;
    verify(&got, &get_text(&format!("{base}/tofu_{ver}_SHA256SUMS"))?, &name)?;
    let out = tmp.join(format!("tofu{EXE}"));
    extract(&archive, &format!("tofu{EXE}"), &out)?;
    let dst = place(&out, bin, "tofu")?;
    let _ = (fs::remove_file(&archive), fs::remove_file(&out));
    log(&format!("Installed OpenTofu {ver} → {}", dst.display()));
    Ok(())
}

/// Pulls one entry (`inner`) out of a `.zip` or `.tar.gz` archive.
fn extract(archive: &Path, inner: &str, out: &Path) -> Result<(), InstallError> {
    let file = File::open(archive).map_err(io_err(archive))?;
    let missing = || InstallError::Failed(format!("{inner} not found in {}", archive.display()));
    let mut dst = File::create(out).map_err(io_err(out))?;
    if archive.extension().is_some_and(|e| e == "zip") {
        let mut zip = zip::ZipArchive::new(file).map_err(|e| InstallError::Failed(e.to_string()))?;
        let mut entry = zip.by_name(inner).map_err(|_| missing())?;
        io::copy(&mut entry, &mut dst).map_err(io_err(out))?;
        return Ok(());
    }
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in tar.entries().map_err(io_err(archive))? {
        let mut entry = entry.map_err(io_err(archive))?;
        if entry.path().is_ok_and(|p| p == Path::new(inner)) {
            io::copy(&mut entry, &mut dst).map_err(io_err(out))?;
            return Ok(());
        }
    }
    Err(missing())
}

/// No console window flashes up when the GUI app starts a console program.
#[inline]
pub(crate) fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs a command, streaming its output lines to `log`.
fn run(cmd: &mut Command, log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    log(&format!("$ {cmd:?}"));
    let mut child = no_window(cmd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| InstallError::Failed(format!("{cmd:?}: {e}")))?;
    // stderr is drained on a thread so a chatty process cannot block on it.
    let err = child.stderr.take().map(|e| std::thread::spawn(move || BufReader::new(e).lines().map_while(Result::ok).collect::<Vec<_>>()));
    if let Some(out) = child.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            log(&line);
        }
    }
    let err_lines = err.and_then(|h| h.join().ok()).unwrap_or_default();
    for l in &err_lines {
        log(l);
    }
    let status = child.wait().map_err(|e| InstallError::Failed(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(InstallError::Failed(err_lines.last().cloned().unwrap_or_else(|| format!("exit status {status}"))))
    }
}

/// First Python ≥ 3.10 on the extended `PATH` (macOS's `/usr/bin/python3`
/// is often 3.9, too old for current ansible-core).
#[cfg(unix)]
fn find_python() -> Option<PathBuf> {
    let path = path_env().unwrap_or_default();
    ["python3.14", "python3.13", "python3.12", "python3.11", "python3.10", "python3"].iter().find_map(|name| {
        std::env::split_paths(&path).map(|d| d.join(name)).filter(|p| p.is_file()).find(|p| {
            Command::new(p)
                .args(["-c", "import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|st| st.success())
        })
    })
}

#[cfg(not(unix))]
fn ansible(_: &Path, _: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    Err(InstallError::Manual("Ansible runs in WSL on Windows"))
}

#[cfg(unix)]
fn ansible(bin: &Path, log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    let venv = bin.with_file_name("ansible");
    let hint = if cfg!(target_os = "macos") {
        "Python 3.10+ is needed: run `xcode-select --install`"
    } else {
        "Python 3.10+ with venv is needed: `sudo apt install python3-venv` or `sudo dnf install python3`"
    };
    let python = find_python().ok_or(InstallError::Failed(hint.to_owned()))?;
    log(&format!("Using {}", python.display()));
    run(Command::new(&python).arg("-m").arg("venv").arg("--clear").arg(&venv), log)
        .map_err(|e| InstallError::Failed(format!("{e}. {hint}")))?;
    let py = venv.join("bin").join("python");
    run(Command::new(&py).args(["-m", "pip", "install", "--upgrade", "--disable-pip-version-check", "ansible-core"]), log)?;
    for prog in ["ansible", "ansible-playbook", "ansible-galaxy"] {
        let link = bin.join(prog);
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(venv.join("bin").join(prog), &link).map_err(io_err(&link))?;
    }
    log(&format!("Installed ansible-core → {}", venv.display()));
    Ok(())
}

/// Script run as root in WSL: no sudo password needed.
const WSL_ANSIBLE: &str = "set -e; export DEBIAN_FRONTEND=noninteractive; \
apt-get update; apt-get install -y python3-venv; \
python3 -m venv --clear /opt/kubernet-wizard/ansible; \
/opt/kubernet-wizard/ansible/bin/pip install --disable-pip-version-check ansible-core; \
for p in ansible ansible-playbook ansible-galaxy; do ln -sf /opt/kubernet-wizard/ansible/bin/$p /usr/local/bin/$p; done";

/// Windows: Ansible inside WSL. Installs WSL + Ubuntu first when no distro
/// answers (admin prompt; Windows usually needs a restart afterwards).
fn ansible_wsl(log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    let distro_ready = no_window(Command::new("wsl").args(["-u", "root", "--", "true"]))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !distro_ready {
        log("No WSL Linux distribution found. Installing WSL + Ubuntu; Windows asks for admin rights.");
        run(
            Command::new("powershell").args([
                "-NoProfile",
                "-Command",
                "Start-Process wsl.exe -ArgumentList '--install','-d','Ubuntu','--no-launch' -Verb RunAs -Wait",
            ]),
            log,
        )?;
        return Err(InstallError::Failed(
            "WSL is installed. Restart Windows, open Ubuntu once from the Start menu to finish its setup, then press Install again".into(),
        ));
    }
    run(Command::new("wsl").args(["-u", "root", "--", "sh", "-c", WSL_ANSIBLE]), log)?;
    log("Installed ansible-core in WSL → /usr/local/bin");
    Ok(())
}

/// Git / OpenSSH through the OS installer.
fn system(program: &str, log: &mut impl FnMut(&str)) -> Result<(), InstallError> {
    if cfg!(target_os = "macos") {
        // Opens the Command Line Tools installer dialog (git, python3, make).
        log("Opening the Apple Command Line Tools installer. Finish it, then press Re-check.");
        return run(Command::new("xcode-select").arg("--install"), log);
    }
    if cfg!(windows) {
        let id = match program {
            "git" => "Git.Git",
            _ => return Err(InstallError::Manual("install it from Windows settings")),
        };
        return run(
            Command::new("winget").args(["install", "-e", "--id", id, "--silent", "--accept-package-agreements", "--accept-source-agreements"]),
            log,
        );
    }
    let has = |p: &str| Command::new("sh").args(["-c", &format!("command -v {p}")]).stdout(Stdio::null()).status().is_ok_and(|s| s.success());
    let (pm, pkg) = match (program, has("apt-get"), has("dnf")) {
        ("git", true, _) => ("apt", "git"),
        ("ssh", true, _) => ("apt", "openssh-client"),
        ("git", _, true) => ("dnf", "git"),
        ("ssh", _, true) => ("dnf", "openssh-clients"),
        _ => return Err(InstallError::Manual("no apt-get or dnf found; install it with your package manager")),
    };
    let script = if pm == "apt" {
        format!("apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y {pkg}")
    } else {
        format!("dnf install -y {pkg}")
    };
    // pkexec shows the desktop password prompt.
    run(Command::new("pkexec").args(["sh", "-c", &script]), log)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_files() {
        let h = "a".repeat(64);
        assert_eq!(expected_sha(&format!("{h}  helm-v3.19.0-linux-amd64.tar.gz\n"), "helm-v3.19.0-linux-amd64.tar.gz"), Some(h.clone()));
        assert_eq!(expected_sha(&format!("{h}\n"), "kubectl"), Some(h.clone()));
        let sums = format!("{}  tofu_1.10.7_darwin_arm64.zip\n{h}  tofu_1.10.7_linux_amd64.zip\n", "b".repeat(64));
        assert_eq!(expected_sha(&sums, "tofu_1.10.7_linux_amd64.zip"), Some(h));
        assert_eq!(expected_sha(&sums, "missing.zip"), None);
    }

    #[test]
    fn hex_encoding() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
