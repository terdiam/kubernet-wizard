//! Deploy step state: generate files, run pipeline, stream logs, Day-2 HA.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use kw_exec::{Event, Run, Step as ExecStep, prereq};
use smallvec::SmallVec;

use super::Wizard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Running,
    Ok(u64),
    Failed(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Planning,
    /// Tofu plan done; waiting for user confirmation.
    AwaitApply,
    Running,
    Finished { ok: bool },
}

const LOG_CAP: usize = 4000;
const LOG_VIEW: usize = 400;
const EVENTS_PER_TICK: usize = 512;

pub struct DeployState {
    pub run: Option<Run>,
    pub phase: Phase,
    pub demo: bool,
    pub steps: SmallVec<[(ExecStep, StepState); 6]>,
    log: VecDeque<SharedString>,
    /// Cached tail text (rebuilt only when lines arrive).
    pub log_text: SharedString,
    pub errors_only: bool,
    pub log_file: Option<PathBuf>,
    pub message: Option<SharedString>,
    pub prereq: Option<Vec<prereq::ToolStatus>>,
    pub ha_input: Entity<TextareaState>,
    poll: Option<Task<()>>,
}

#[inline]
fn is_error_line(line: &str) -> bool {
    let l = line.as_bytes();
    // cheap ASCII case-insensitive contains for common markers
    ["fatal", "error", "failed!", "unreachable"].iter().any(|m| {
        l.windows(m.len()).any(|w| w.eq_ignore_ascii_case(m.as_bytes()))
    })
}

impl DeployState {
    pub fn new(ha_input: Entity<TextareaState>) -> Self {
        Self {
            run: None,
            phase: Phase::Idle,
            demo: false,
            steps: SmallVec::new(),
            log: VecDeque::with_capacity(LOG_CAP),
            log_text: SharedString::default(),
            errors_only: false,
            log_file: None,
            message: None,
            prereq: None,
            ha_input,
            poll: None,
        }
    }

    #[inline]
    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Planning | Phase::Running)
    }

    fn push_line(&mut self, line: String) {
        if self.log.len() == LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(line.into());
    }

    pub fn rebuild_log_text(&mut self) {
        let lines: SmallVec<[&SharedString; 64]> = if self.errors_only {
            self.log.iter().filter(|l| is_error_line(l)).collect()
        } else {
            self.log.iter().collect()
        };
        let tail = &lines[lines.len().saturating_sub(LOG_VIEW)..];
        let mut text = String::with_capacity(tail.iter().map(|l| l.len() + 1).sum());
        for l in tail {
            text.push_str(l);
            text.push('\n');
        }
        self.log_text = text.into();
    }

    fn set_step(&mut self, step: ExecStep, state: StepState) {
        match self.steps.iter_mut().find(|(s, _)| *s == step) {
            Some(slot) => slot.1 = state,
            None => self.steps.push((step, state)),
        }
    }
}

impl Wizard {
    pub(super) fn check_prereq(&mut self, cx: &mut Context<Self>) {
        let cloud = self.spec.tofu_provider().is_some();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { prereq::check_all(cloud) }).await;
            let _ = this.update(cx, |w, cx| {
                w.deploy.prereq = Some(res);
                cx.notify();
            });
        })
        .detach();
    }

    /// Writes the project. Returns false on failure (message set).
    pub(super) fn generate(&mut self, cx: &mut Context<Self>) -> bool {
        let res = kw_render::render_spec(&self.spec)
            .map_err(|e| e.to_string())
            .and_then(|files| {
                kw_render::write_all(&files, &self.project_dir, self.spec.tofu_provider().is_some()).map_err(|e| e.to_string())
            });
        self.deploy.message = Some(match &res {
            Ok(n) => format!("{n} files → {}", self.project_dir.display()).into(),
            Err(e) => format!("✕ {e}").into(),
        });
        if res.is_ok() {
            self.existing = true;
        }
        cx.notify();
        res.is_ok()
    }

    fn start_run(&mut self, run: Run, steps: &[ExecStep], phase: super::deploy::Phase, cx: &mut Context<Self>) {
        self.deploy.steps.retain(|(s, _)| !steps.contains(s));
        self.deploy.steps.extend(steps.iter().map(|&s| (s, StepState::Pending)));
        self.deploy.run = Some(run);
        self.deploy.phase = phase;
        self.deploy.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(80)).await;
                let keep = this.update(cx, |w, cx| w.drain_events(cx)).unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub(super) fn start_demo(&mut self, cx: &mut Context<Self>) {
        let mut steps: SmallVec<[ExecStep; 6]> = SmallVec::from_slice(kw_exec::plan_steps(self.spec.tofu_provider()));
        steps.extend_from_slice(kw_exec::apply_steps(self.spec.tofu_provider()));
        self.deploy.demo = true;
        self.deploy.steps.clear();
        let run = kw_exec::start_demo(self.spec.tofu_provider());
        self.start_run(run, &steps, Phase::Running, cx);
    }

    /// Real deploy: generate, then tofu plan (if any) or apply directly.
    pub(super) fn start_deploy(&mut self, cx: &mut Context<Self>) {
        if !self.generate(cx) {
            return;
        }
        self.deploy.demo = false;
        self.deploy.steps.clear();
        let plan = kw_exec::plan_steps(self.spec.tofu_provider());
        if plan.is_empty() {
            self.start_apply(cx);
        } else {
            let run = kw_exec::start(&self.project_dir, self.spec.tofu_provider(), plan);
            self.start_run(run, plan, Phase::Planning, cx);
        }
    }

    pub(super) fn start_apply(&mut self, cx: &mut Context<Self>) {
        let steps = kw_exec::apply_steps(self.spec.tofu_provider());
        let run = kw_exec::start(&self.project_dir, self.spec.tofu_provider(), steps);
        self.start_run(run, steps, Phase::Running, cx);
    }

    pub(super) fn start_upgrade_ha(&mut self, cx: &mut Context<Self>) {
        const STEPS: [ExecStep; 3] = [ExecStep::AnsibleDeps, ExecStep::AnsibleUpgradeHa, ExecStep::Smoke];
        let text = self.deploy.ha_input.read(cx).value();
        let nodes = match super::parse_nodes(&text) {
            Ok(n) => n,
            Err(e) => {
                self.deploy.message = Some(format!("✕ {e}").into());
                cx.notify();
                return;
            }
        };
        let target = u8::try_from(nodes.len()).unwrap_or(u8::MAX).saturating_add(self.spec.topology.control_planes);
        match kw_core::ha::plan_upgrade(&self.spec, &nodes, target) {
            Ok(next) => {
                self.spec = next;
                self.revalidate();
                if !self.generate(cx) {
                    return;
                }
                self.deploy.demo = false;
                let run = kw_exec::start(&self.project_dir, self.spec.tofu_provider(), &STEPS);
                self.start_run(run, &STEPS, Phase::Running, cx);
            }
            Err(e) => {
                self.deploy.message = Some(format!("✕ {e}").into());
                cx.notify();
            }
        }
    }

    pub(super) fn cancel_run(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = &self.deploy.run {
            run.cancel();
        }
        cx.notify();
    }

    /// Pulls pending events. Returns false when polling should stop.
    fn drain_events(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(run) = self.deploy.run.take() else { return false };
        let mut got_lines = false;
        let mut done = None;
        let mut changed = false;
        for _ in 0..EVENTS_PER_TICK {
            match run.events.try_recv() {
                Ok(Event::Line { text, .. }) => {
                    self.deploy.push_line(text);
                    got_lines = true;
                }
                Ok(Event::StepStarted(s)) => {
                    self.deploy.set_step(s, StepState::Running);
                    self.deploy.push_line(format!("==> {}", s.label()));
                    changed = true;
                }
                Ok(Event::StepFinished { step, ok, secs }) => {
                    self.deploy.set_step(step, if ok { StepState::Ok(secs) } else { StepState::Failed(secs) });
                    changed = true;
                }
                Ok(Event::Done { failed, cancelled, log }) => {
                    done = Some((failed.is_none() && !cancelled, log));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = Some((false, None));
                    break;
                }
            }
        }
        if got_lines || changed {
            self.deploy.rebuild_log_text();
        }
        if let Some((ok, log)) = done {
            drop(run);
            self.deploy.log_file = log;
            self.deploy.phase = match self.deploy.phase {
                Phase::Planning if ok => Phase::AwaitApply,
                _ => Phase::Finished { ok },
            };
            cx.notify();
            return false;
        }
        self.deploy.run = Some(run);
        if got_lines || changed {
            cx.notify();
        }
        true
    }
}
