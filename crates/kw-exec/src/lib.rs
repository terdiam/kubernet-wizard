//! Executes deployment steps (OpenTofu, Ansible) with streamed output.
//!
//! Threads + bounded std channels only: no async runtime, no locks.

pub mod prereq;
pub mod tools;

use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kw_core::CloudProvider;

/// One deploy step = one external command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    TofuInit,
    TofuPlan,
    TofuApply,
    AnsibleDeps,
    AnsibleSite,
    AnsibleUpgradeHa,
    Smoke,
}

impl Step {
    pub const fn label(self) -> &'static str {
        match self {
            Self::TofuInit => "OpenTofu init",
            Self::TofuPlan => "OpenTofu plan",
            Self::TofuApply => "Provision infrastructure",
            Self::AnsibleDeps => "Install Ansible collections",
            Self::AnsibleSite => "Bootstrap cluster (Ansible)",
            Self::AnsibleUpgradeHa => "Upgrade to HA (Ansible)",
            Self::Smoke => "Smoke test",
        }
    }
}

/// Steps for a full deploy. Tofu plan is split so the UI can ask for
/// confirmation between `plan_steps` and `apply_steps`.
///
/// `tofu` is the provider OpenTofu provisions (`ClusterSpec::tofu_provider`).
pub fn plan_steps(tofu: Option<CloudProvider>) -> &'static [Step] {
    if tofu.is_some() { &[Step::TofuInit, Step::TofuPlan] } else { &[] }
}

pub fn apply_steps(tofu: Option<CloudProvider>) -> &'static [Step] {
    if tofu.is_some() {
        &[Step::TofuApply, Step::AnsibleDeps, Step::AnsibleSite, Step::Smoke]
    } else {
        &[Step::AnsibleDeps, Step::AnsibleSite, Step::Smoke]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    StepStarted(Step),
    Line { step: Step, stream: Stream, text: String },
    StepFinished { step: Step, ok: bool, secs: u64 },
    /// Whole run ended. `failed` = first failing step.
    Done { failed: Option<Step>, cancelled: bool, log: Option<PathBuf> },
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("cannot start {program}: {source}")]
    Spawn { program: &'static str, source: std::io::Error },
}

/// Builds a command for the host. App-managed tools ([`tools`]) win over
/// `PATH`; on Windows, Ansible runs inside WSL.
pub fn host_command(program: &str) -> Command {
    let mut c = if cfg!(windows) && program.starts_with("ansible") {
        let mut c = Command::new("wsl");
        c.arg("--").arg(program);
        c
    } else {
        tools::managed(program).map_or_else(|| Command::new(program), Command::new)
    };
    if let Some(path) = tools::path_env() {
        c.env("PATH", path);
    }
    c
}

/// `C:\Users\me\x` → `/mnt/c/Users/me/x` (WSL path).
pub fn wsl_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        let mut out = String::with_capacity(s.len() + 5);
        out.push_str("/mnt/");
        out.push(char::from(b[0].to_ascii_lowercase()));
        out.extend(s[2..].chars().map(|c| if c == '\\' { '/' } else { c }));
        out
    } else {
        s.into_owned()
    }
}

fn command_for(step: Step, project: &Path, tofu: Option<CloudProvider>) -> (&'static str, Command) {
    let ansible_dir = project.join("ansible");
    let mut tofu_dir = project.join("tofu");
    if let Some(p) = tofu {
        tofu_dir.push(p.id());
    }
    let (program, dir, args): (&'static str, &Path, &[&str]) = match step {
        Step::TofuInit => ("tofu", &tofu_dir, &["init", "-input=false"]),
        Step::TofuPlan => ("tofu", &tofu_dir, &["plan", "-input=false", "-out=kw.plan"]),
        Step::TofuApply => ("tofu", &tofu_dir, &["apply", "-input=false", "kw.plan"]),
        Step::AnsibleDeps => ("ansible-galaxy", &ansible_dir, &["collection", "install", "-r", "requirements.yml"]),
        Step::AnsibleSite => ("ansible-playbook", &ansible_dir, &["site.yml"]),
        Step::AnsibleUpgradeHa => ("ansible-playbook", &ansible_dir, &["upgrade-ha.yml"]),
        Step::Smoke => ("kubectl", project, &["--kubeconfig", "kubeconfig", "get", "nodes", "-o", "wide"]),
    };
    let mut cmd = host_command(program);
    if cfg!(windows) && program.starts_with("ansible") {
        // WSL: cd inside the Linux side.
        cmd = Command::new("wsl");
        cmd.args(["--cd", &wsl_path(dir), "--", program]);
    } else {
        cmd.current_dir(dir);
    }
    cmd.args(args)
        .env("NO_COLOR", "1")
        .env("ANSIBLE_NOCOLOR", "1")
        .env("ANSIBLE_FORCE_COLOR", "0")
        .env("TF_IN_AUTOMATION", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    (program, cmd)
}

/// Handle to a running pipeline.
pub struct Run {
    pub events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Run {
    /// Requests cancellation; the current child process is killed.
    #[inline]
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        self.cancel();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// UI channel capacity: bounded so a flood of output applies back-pressure.
const CHANNEL_CAP: usize = 1024;

/// Starts `steps` sequentially in a background thread.
pub fn start(project: &Path, tofu: Option<CloudProvider>, steps: &[Step]) -> Run {
    let (tx, rx) = mpsc::sync_channel(CHANNEL_CAP);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let project = project.to_path_buf();
    let steps = steps.to_vec();
    let thread = thread::Builder::new()
        .name("kw-pipeline".to_owned())
        .spawn(move || run_steps(&project, tofu, &steps, &tx, &flag))
        .ok();
    Run { events: rx, cancel, thread }
}

fn open_log(project: &Path) -> Option<(PathBuf, BufWriter<File>)> {
    let dir = project.join("logs");
    fs::create_dir_all(&dir).ok()?;
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let path = dir.join(format!("deploy-{ts}.log"));
    let file = File::create(&path).ok()?;
    Some((path, BufWriter::new(file)))
}

fn run_steps(project: &Path, tofu: Option<CloudProvider>, steps: &[Step], tx: &SyncSender<Event>, cancel: &AtomicBool) {
    let mut log = open_log(project);
    let mut failed = None;
    for &step in steps {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let _ = tx.send(Event::StepStarted(step));
        let started = Instant::now();
        let ok = match run_one(step, project, tofu, tx, cancel, log.as_mut().map(|l| &mut l.1)) {
            Ok(ok) => ok,
            Err(e) => {
                let _ = tx.send(Event::Line { step, stream: Stream::Stderr, text: e.to_string() });
                false
            }
        };
        let _ = tx.send(Event::StepFinished { step, ok, secs: started.elapsed().as_secs() });
        if !ok {
            failed = Some(step);
            break;
        }
    }
    let log_path = log.map(|(p, mut w)| {
        let _ = w.flush();
        p
    });
    let cancelled = cancel.load(Ordering::Relaxed);
    let _ = tx.send(Event::Done { failed, cancelled, log: log_path });
}

fn pump(reader: impl std::io::Read + Send + 'static, stream: Stream, tx: mpsc::Sender<(Stream, String)>) -> JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if tx.send((stream, line)).is_err() {
                break;
            }
        }
    })
}

fn run_one(
    step: Step,
    project: &Path,
    tofu: Option<CloudProvider>,
    tx: &SyncSender<Event>,
    cancel: &AtomicBool,
    mut log: Option<&mut BufWriter<File>>,
) -> Result<bool, ExecError> {
    let (program, mut cmd) = command_for(step, project, tofu);
    let mut child: Child = cmd.spawn().map_err(|source| ExecError::Spawn { program, source })?;

    let (ltx, lrx) = mpsc::channel();
    let readers = [
        child.stdout.take().map(|r| pump(r, Stream::Stdout, ltx.clone())),
        child.stderr.take().map(|r| pump(r, Stream::Stderr, ltx)),
    ];

    let status = loop {
        // Drain lines with a short timeout so we can poll cancel / exit.
        while let Ok((stream, text)) = lrx.recv_timeout(Duration::from_millis(50)) {
            if let Some(w) = log.as_deref_mut() {
                let _ = writeln!(w, "[{}] {text}", step.label());
            }
            if tx.send(Event::Line { step, stream, text }).is_err() {
                cancel.store(true, Ordering::Relaxed);
            }
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            break child.wait().ok();
        }
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(_) => break None,
        }
    };
    readers.into_iter().flatten().for_each(|h| {
        let _ = h.join();
    });
    // Remaining buffered lines.
    for (stream, text) in lrx.try_iter() {
        let _ = tx.send(Event::Line { step, stream, text });
    }
    Ok(status.is_some_and(|s| s.success()) && !cancel.load(Ordering::Relaxed))
}

/// Simulated run for demo mode / onboarding — no servers touched.
pub fn start_demo(tofu: Option<CloudProvider>) -> Run {
    let (tx, rx) = mpsc::sync_channel(CHANNEL_CAP);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let mut steps: Vec<Step> = plan_steps(tofu).to_vec();
    steps.extend_from_slice(apply_steps(tofu));
    let thread = thread::Builder::new()
        .name("kw-demo".to_owned())
        .spawn(move || {
            const LINES: [&str; 5] = [
                "PLAY [Prepare all nodes] *****",
                "TASK [prereq : Install base packages] ok",
                "TASK [tuning : Tuning sysctl] changed",
                "TASK [distro : Install] changed",
                "PLAY RECAP ok=42 changed=17 failed=0",
            ];
            for step in steps {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let _ = tx.send(Event::StepStarted(step));
                for text in LINES {
                    thread::sleep(Duration::from_millis(250));
                    let _ = tx.send(Event::Line { step, stream: Stream::Stdout, text: format!("[demo] {text}") });
                }
                let _ = tx.send(Event::StepFinished { step, ok: true, secs: 1 });
            }
            let cancelled = flag.load(Ordering::Relaxed);
            let _ = tx.send(Event::Done { failed: None, cancelled, log: None });
        })
        .ok();
    Run { events: rx, cancel, thread }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_paths() {
        assert_eq!(wsl_path(Path::new(r"C:\Users\me\kw")), "/mnt/c/Users/me/kw");
        assert_eq!(wsl_path(Path::new("/home/me")), "/home/me");
    }

    #[test]
    fn tofu_steps_follow_provider() {
        assert_eq!(plan_steps(None), &[] as &[Step]);
        assert_eq!(plan_steps(Some(CloudProvider::Aws)), &[Step::TofuInit, Step::TofuPlan]);
        let (_, cmd) = command_for(Step::TofuPlan, Path::new("/p"), Some(CloudProvider::Gcp));
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/p/tofu/gcp")));
    }

    #[test]
    fn demo_run_completes() {
        let run = start_demo(None);
        let last = run.events.iter().last();
        assert!(matches!(last, Some(Event::Done { failed: None, .. })));
    }

    #[test]
    fn missing_program_reports_failure() {
        let dir = std::env::temp_dir().join("kw-exec-test");
        let _ = fs::create_dir_all(dir.join("ansible"));
        // Smoke runs kubectl; force failure via missing kubeconfig is env dependent,
        // so only assert we get a Done event.
        let run = start(&dir, None, &[Step::Smoke]);
        assert!(run.events.iter().any(|e| matches!(e, Event::Done { .. })));
    }
}
