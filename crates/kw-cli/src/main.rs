//! `kw` — headless kubernet-wizard: validate, generate, deploy, upgrade-ha.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use kw_core::{ClusterSpec, Severity, presets::Preset};
use kw_exec::{Event, Step, prereq};

const USAGE: &str = "\
kw — kubernet-wizard CLI

USAGE:
  kw presets                                  list presets
  kw preset <index> [--out FILE]              print/save preset spec (cluster.yaml)
  kw validate --spec FILE
  kw generate --spec FILE --out DIR
  kw deploy   --spec FILE --out DIR [--yes] [--demo]
  kw upgrade-ha --out DIR
  kw prereq [--cloud]
";

#[derive(Default)]
struct Args {
    cmd: String,
    pos: Option<String>,
    spec: Option<PathBuf>,
    out: Option<PathBuf>,
    yes: bool,
    demo: bool,
    cloud: bool,
}

fn parse() -> Result<Args> {
    use lexopt::prelude::*;
    let mut a = Args::default();
    let mut p = lexopt::Parser::from_env();
    while let Some(arg) = p.next()? {
        match arg {
            Long("spec") | Short('s') => a.spec = Some(p.value()?.into()),
            Long("out") | Short('o') => a.out = Some(p.value()?.into()),
            Long("yes") | Short('y') => a.yes = true,
            Long("demo") => a.demo = true,
            Long("cloud") => a.cloud = true,
            Long("help") | Short('h') => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            Value(v) if a.cmd.is_empty() => a.cmd = v.string()?,
            Value(v) => a.pos = Some(v.string()?),
            _ => return Err(arg.unexpected().into()),
        }
    }
    Ok(a)
}

fn load(path: Option<&Path>) -> Result<ClusterSpec> {
    let path = path.context("--spec FILE is required")?;
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    ClusterSpec::from_yaml(&text).with_context(|| format!("parse {}", path.display()))
}

/// Prints issues; returns false when errors exist.
fn report(spec: &ClusterSpec) -> bool {
    let r = kw_distro::validate(spec);
    for i in &r.issues {
        let tag = if i.severity == Severity::Error { "error" } else { "warn " };
        eprintln!("{tag} [{:?}] {}", i.field, i.message);
    }
    !r.has_errors()
}

fn generate(spec: &ClusterSpec, out: &Path) -> Result<()> {
    if !report(spec) {
        bail!("spec has errors");
    }
    let files = kw_render::render_spec(spec)?;
    let n = kw_render::write_all(&files, out, kw_exec::uses_tofu(spec))?;
    println!("wrote {n} files to {}", out.display());
    Ok(())
}

fn stream(run: &kw_exec::Run) -> bool {
    let mut ok = true;
    for ev in &run.events {
        match ev {
            Event::StepStarted(s) => println!("==> {}", s.label()),
            Event::Line { text, .. } => println!("    {text}"),
            Event::StepFinished { step, ok: good, secs } => {
                println!("<== {} {} ({secs}s)", step.label(), if good { "OK" } else { "FAILED" });
            }
            Event::Done { failed, cancelled, log } => {
                ok = failed.is_none() && !cancelled;
                if let Some(l) = log {
                    println!("log: {}", l.display());
                }
            }
        }
    }
    ok
}

fn confirm(prompt: &str) -> bool {
    print!("{prompt} [y/N] ");
    let _ = io::stdout().flush();
    let mut line = String::with_capacity(4);
    io::stdin().lock().read_line(&mut line).is_ok() && matches!(line.trim(), "y" | "Y" | "yes")
}

fn run() -> Result<bool> {
    let a = parse()?;
    match a.cmd.as_str() {
        "presets" => {
            for (i, p) in Preset::ALL.iter().enumerate() {
                println!("{i}  {:<24} {}", p.title(), p.summary());
            }
        }
        "preset" => {
            let idx: usize = a.pos.as_deref().unwrap_or("0").parse()?;
            let p = Preset::ALL.get(idx).context("unknown preset index")?;
            let yaml = p.build().to_yaml()?;
            match a.out {
                Some(f) => fs::write(&f, yaml).with_context(|| format!("write {}", f.display()))?,
                None => print!("{yaml}"),
            }
        }
        "validate" => {
            let ok = report(&load(a.spec.as_deref())?);
            println!("{}", if ok { "valid" } else { "invalid" });
            return Ok(ok);
        }
        "generate" => generate(&load(a.spec.as_deref())?, &a.out.context("--out DIR is required")?)?,
        "deploy" => {
            let spec = load(a.spec.as_deref())?;
            if a.demo {
                return Ok(stream(&kw_exec::start_demo(&spec)));
            }
            let out = a.out.context("--out DIR is required")?;
            generate(&spec, &out)?;
            let plan = kw_exec::plan_steps(&spec);
            if !plan.is_empty() && !stream(&kw_exec::start(&out, &spec, plan)) {
                return Ok(false);
            }
            if !a.yes && !confirm("Apply changes to real servers?") {
                println!("aborted");
                return Ok(false);
            }
            return Ok(stream(&kw_exec::start(&out, &spec, kw_exec::apply_steps(&spec))));
        }
        "upgrade-ha" => {
            let out = a.out.context("--out DIR is required")?;
            let spec = load(Some(&out.join("cluster.yaml")))?;
            if !spec.topology.ha {
                bail!("edit cluster.yaml: set topology.ha=true, control_planes=3 and add the new nodes, or use the app's Upgrade to HA screen");
            }
            generate(&spec, &out)?;
            return Ok(stream(&kw_exec::start(&out, &spec, &[Step::AnsibleDeps, Step::AnsibleUpgradeHa, Step::Smoke])));
        }
        "prereq" => {
            let mut ok = true;
            for s in prereq::check_all(a.cloud) {
                let (mark, detail) = match s.state {
                    prereq::ToolState::Ok { version: (ma, mi) } => ("ok  ", format!("{ma}.{mi}")),
                    prereq::ToolState::Outdated { version: (ma, mi) } => {
                        ok = false;
                        ("old ", format!("{ma}.{mi} < {}.{} → {}", s.tool.min.0, s.tool.min.1, s.tool.install.current()))
                    }
                    prereq::ToolState::Missing => {
                        ok = false;
                        ("miss", format!("install: {}", s.tool.install.current()))
                    }
                };
                println!("{mark} {:<9} {detail}", s.tool.name);
            }
            return Ok(ok);
        }
        _ => {
            print!("{USAGE}");
            return Ok(a.cmd.is_empty());
        }
    }
    Ok(true)
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
