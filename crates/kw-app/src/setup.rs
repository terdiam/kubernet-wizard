//! First-run / setup screen: required tools must be present before the app
//! opens; missing ones install with one click into the app's tools folder.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_exec::prereq::{Tool, ToolState, ToolStatus};
use kw_exec::tools::{self, Method};

use crate::app::KwApp;
use crate::assets;
use crate::i18n::{self, K};
use crate::settings::Ui;
use crate::theme::pal;

const LOG_CAP: usize = 200;
const CHANNEL_CAP: usize = 256;

pub(crate) enum Msg {
    Start(&'static str),
    Line(String),
    Failed(&'static str, String),
    Finished,
}

/// Install job running on a worker thread.
pub(crate) struct Installing {
    current: Option<&'static str>,
    rx: Receiver<Msg>,
}

#[derive(Default)]
pub(crate) struct SetupState {
    pub(crate) job: Option<Installing>,
    pub(crate) log: VecDeque<SharedString>,
    pub(crate) error: Option<SharedString>,
}

impl SetupState {
    fn push(&mut self, line: impl Into<SharedString>) {
        if self.log.len() == LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(line.into());
    }
}

/// Required (non cloud-only) tools that are missing or too old.
#[inline]
fn missing_required(list: &[ToolStatus]) -> bool {
    list.iter().any(|s| !s.is_ok() && !s.tool.cloud_only)
}

impl KwApp {
    /// Setup screen stays up while the check runs or required tools are missing.
    pub(crate) fn setup_blocked(&self) -> bool {
        self.prereq.as_deref().is_none_or(missing_required)
    }

    pub(crate) fn install_tools(&mut self, list: Vec<Tool>, cx: &mut Context<Self>) {
        if self.setup.job.is_some() || list.is_empty() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(CHANNEL_CAP);
        let spawned = std::thread::Builder::new().name("kw-install".to_owned()).spawn(move || {
            for t in &list {
                let _ = tx.send(Msg::Start(t.name));
                if let Err(e) = tools::install(t, |l| {
                    let _ = tx.send(Msg::Line(l.to_owned()));
                }) {
                    let _ = tx.send(Msg::Failed(t.name, e.to_string()));
                }
            }
            let _ = tx.send(Msg::Finished);
        });
        if let Err(e) = spawned {
            self.setup.error = Some(e.to_string().into());
            return;
        }
        self.setup.error = None;
        self.setup.job = Some(Installing { current: None, rx });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(120)).await;
                let keep = this.update(cx, |app, cx| app.drain_install(cx)).unwrap_or(false);
                if !keep {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Pulls worker messages. Returns false once the job is over.
    fn drain_install(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(mut job) = self.setup.job.take() else { return false };
        let mut finished = false;
        loop {
            match job.rx.try_recv() {
                Ok(Msg::Start(name)) => {
                    job.current = Some(name);
                    self.setup.push(format!("==> {name}"));
                }
                Ok(Msg::Line(l)) => self.setup.push(l),
                Ok(Msg::Failed(name, e)) => {
                    self.setup.push(format!("✕ {name}: {e}"));
                    self.setup.error = Some(format!("{name}: {e}").into());
                }
                Ok(Msg::Finished) | Err(TryRecvError::Disconnected) => {
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        if finished {
            self.recheck_prereq(cx);
        } else {
            self.setup.job = Some(job);
        }
        cx.notify();
        !finished
    }

    pub(crate) fn recheck_prereq(&mut self, cx: &mut Context<Self>) {
        self.prereq = None;
        self.check_prereq(cx);
    }

    fn auto_installable(s: &ToolStatus) -> bool {
        !s.is_ok() && !matches!(tools::method(&s.tool), Method::Manual(_))
    }

    pub(crate) fn setup_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = cx.global::<Ui>().prefs.lang;
        let p = pal(cx);
        let mono = cx.theme().mono_font_family.clone();
        let list = self.prereq.as_deref().unwrap_or_default();
        let checking = self.prereq.is_none();
        let busy = self.setup.job.is_some();
        let current = self.setup.job.as_ref().and_then(|j| j.current);
        let blocked = self.setup_blocked();
        let ready = list.iter().filter(|s| s.is_ok()).count();
        let missing: Vec<Tool> = list.iter().filter(|s| Self::auto_installable(s)).map(|s| s.tool).collect();
        let (title, body) = if self.welcome {
            (K::WelcomeTitle, K::WelcomeBody)
        } else {
            (K::SetupTitle, K::SetupBody)
        };

        let rows = list.iter().map(|s| {
            let (icon, bg, fg, label, ver) = match s.state {
                ToolState::Ok { version } => (IconName::CircleCheck, p.oks, p.okt, K::StDone, format!("{}.{}", version.0, version.1)),
                ToolState::Outdated { version } => (IconName::TriangleAlert, p.wns, p.wnt, K::Outdated, format!("{}.{}", version.0, version.1)),
                ToolState::Missing if s.tool.cloud_only => (IconName::TriangleAlert, p.wns, p.wnt, K::Optional, "—".to_owned()),
                ToolState::Missing => (IconName::CircleX, p.ers, p.ert, K::Missing, "—".to_owned()),
            };
            let tool = s.tool;
            let action: AnyElement = if s.is_ok() {
                div().into_any_element()
            } else if current == Some(tool.name) {
                h_flex()
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .text_color(p.prt)
                    .child(Icon::new(IconName::Loader).xsmall())
                    .child(i18n::t(lang, K::Installing))
                    .into_any_element()
            } else if let Method::Manual(hint) = tools::method(&tool) {
                div().text_xs().text_color(p.mu).text_right().child(hint).into_any_element()
            } else {
                Button::new(SharedString::from(format!("install-{}", tool.program)))
                    .small()
                    .outline()
                    .icon(IconName::ArrowDown)
                    .label(i18n::t(lang, K::Install))
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.install_tools(vec![tool], cx)))
                    .into_any_element()
            };
            h_flex()
                .gap_3()
                .items_center()
                .px(px(18.))
                .py(px(10.))
                .border_t_1()
                .border_color(p.bd)
                .text_sm()
                .child(div().w(px(110.)).font_semibold().child(tool.name))
                .child(div().w(px(60.)).text_xs().text_color(p.mu).font_family(mono.clone()).child(ver))
                .child(
                    h_flex()
                        .flex_none()
                        .gap_1()
                        .items_center()
                        .px(px(9.))
                        .py(px(2.))
                        .rounded_full()
                        .bg(bg)
                        .text_color(fg)
                        .text_xs()
                        .font_semibold()
                        .child(Icon::new(icon).xsmall())
                        .child(i18n::t(lang, label)),
                )
                .child(h_flex().flex_1().justify_end().child(action))
        });

        let card = v_flex()
            .w(px(680.))
            .mt_1()
            .rounded(px(12.))
            .border_1()
            .border_color(p.bd)
            .bg(p.sf)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px(px(18.))
                    .py(px(12.))
                    .child(div().text_sm().font_bold().child(i18n::t(lang, K::Prereq)))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .when(!list.is_empty(), |d| d.child(div().text_xs().text_color(p.mu).child(format!("{ready} / {}", list.len()))))
                            .child(
                                Button::new("recheck")
                                    .ghost()
                                    .small()
                                    .icon(IconName::RefreshCw)
                                    .label(i18n::t(lang, K::Recheck))
                                    .disabled(busy || checking)
                                    .on_click(cx.listener(|this, _, _, cx| this.recheck_prereq(cx))),
                            )
                            .when(!missing.is_empty(), |d| {
                                d.child(
                                    Button::new("install-all")
                                        .primary()
                                        .small()
                                        .icon(IconName::ArrowDown)
                                        .label(i18n::t(lang, K::InstallAll))
                                        .disabled(busy)
                                        .on_click(cx.listener(move |this, _, _, cx| this.install_tools(missing.clone(), cx))),
                                )
                            }),
                    ),
            )
            .when(checking, |d| {
                d.child(
                    h_flex()
                        .gap_2()
                        .px(px(18.))
                        .py(px(10.))
                        .border_t_1()
                        .border_color(p.bd)
                        .text_sm()
                        .text_color(p.mu)
                        .child(Icon::new(IconName::Loader).small())
                        .child(i18n::t(lang, K::Checking)),
                )
            })
            .children(rows)
            .child(
                h_flex()
                    .gap_2()
                    .px(px(18.))
                    .py(px(10.))
                    .border_t_1()
                    .border_color(p.bd)
                    .text_xs()
                    .text_color(p.mu)
                    .child(Icon::new(IconName::Info).xsmall())
                    .child(i18n::t(lang, K::InstallNote)),
            );

        let log = (!self.setup.log.is_empty()).then(|| {
            v_flex()
                .id("install-log")
                .w(px(680.))
                .max_h(px(180.))
                .p_3()
                .rounded(px(10.))
                .bg(gpui_kit::rgb(0x0010_1528))
                .text_color(gpui_kit::rgb(0x00C9_D1E8))
                .font_family(mono.clone())
                .text_xs()
                .overflow_y_scroll()
                .children(self.setup.log.iter().cloned())
        });

        let gate_tip = i18n::t(lang, K::NeedTools);
        let buttons = if self.welcome {
            h_flex()
                .gap(px(10.))
                .child(
                    Button::new("w-tour")
                        .primary()
                        .icon(IconName::Map)
                        .label(i18n::t(lang, K::StartTour))
                        .disabled(blocked)
                        .when(blocked, |b| b.tooltip(gate_tip))
                        .on_click(cx.listener(|this, _, _, cx| this.finish_welcome(true, cx))),
                )
                .child(
                    Button::new("w-skip")
                        .ghost()
                        .label(i18n::t(lang, K::Skip))
                        .disabled(blocked)
                        .when(blocked, |b| b.tooltip(gate_tip))
                        .on_click(cx.listener(|this, _, _, cx| this.finish_welcome(false, cx))),
                )
        } else {
            h_flex().child(
                Button::new("setup-continue")
                    .primary()
                    .icon(IconName::ArrowRight)
                    .label(i18n::t(lang, K::Continue))
                    .disabled(blocked)
                    .when(blocked, |b| b.tooltip(gate_tip))
                    .on_click(cx.listener(|_, _, _, cx| cx.notify())),
            )
        };

        v_flex()
            .id("setup")
            .size_full()
            .items_center()
            .px_6()
            .py(px(32.))
            .gap(px(16.))
            .bg(p.bg)
            .overflow_y_scroll()
            .child(img(assets::LOGO).size(px(80.)))
            .child(div().text_3xl().font_bold().child(i18n::t(lang, title)))
            .child(div().max_w(px(580.)).text_center().text_color(p.mu).child(i18n::t(lang, body)))
            .child(card)
            .children(log)
            .when_some(self.setup.error.clone(), |d, e| {
                d.child(
                    h_flex()
                        .w(px(680.))
                        .gap_2()
                        .text_sm()
                        .text_color(p.ert)
                        .child(Icon::new(IconName::CircleX).small())
                        .child(e),
                )
            })
            .when(blocked && !checking, |d| {
                d.child(div().text_sm().text_color(p.ert).child(gate_tip))
            })
            .child(buttons)
    }
}
