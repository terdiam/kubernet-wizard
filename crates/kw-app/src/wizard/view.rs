//! Wizard shell: stepper, content frame, context panel, footer + widgets.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_core::{Field, Severity, Step};

use super::{Wizard, WizardEvent};
use crate::docs::DocPage;
use crate::help::{self, Topic};
use crate::i18n::{self, K, Lang};
use crate::settings::Ui;
use crate::theme::{Palette, pal};

/// Highlight ring when the tour points at `target`.
pub(super) fn tour_ring<E: Styled>(el: E, target: &'static str, cx: &App) -> E {
    if cx.global::<Ui>().tour_target() == Some(target) {
        el.border_2().border_color(cx.theme().ring).rounded_md()
    } else {
        el
    }
}

impl Wizard {
    #[inline]
    pub(super) fn lang(cx: &App) -> Lang {
        cx.global::<Ui>().prefs.lang
    }

    #[inline]
    pub(super) fn advanced(cx: &App) -> bool {
        cx.global::<Ui>().prefs.advanced
    }

    /// Labeled field with ⓘ docs button and inline issues.
    pub(super) fn field(
        &self,
        id: &'static str,
        label: &'static str,
        field: Option<Field>,
        doc: Option<DocPage>,
        control: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme();
        let (danger, warning, muted) = (theme.danger, theme.warning, theme.muted_foreground);
        let issues = field.map_or_else(smallvec::SmallVec::<[_; 2]>::new, |f| self.issues_for(f).collect());
        let first_help = id == "f-name";
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(div().text_sm().font_medium().child(label))
                    .when_some(doc, |row, doc| {
                        let btn = Button::new(id)
                            .ghost()
                            .xsmall()
                            .icon(IconName::Info)
                            .tooltip(i18n::t(Self::lang(cx), K::Learn))
                            .on_click(cx.listener(move |_, _, _, cx| cx.emit(WizardEvent::OpenDocs(doc))));
                        let wrapped = div().id(if first_help { "help-button" } else { id }).child(btn);
                        row.child(if first_help { tour_ring(wrapped, "help-button", cx) } else { wrapped })
                    }),
            )
            .child(control)
            .children(issues.into_iter().map(|i| {
                let (color, icon) = if i.severity == Severity::Error {
                    (danger, IconName::CircleX)
                } else {
                    (warning, IconName::TriangleAlert)
                };
                h_flex()
                    .gap_1()
                    .items_start()
                    .text_xs()
                    .text_color(color)
                    .child(Icon::new(icon).xsmall())
                    .child(div().child(i.message.to_string()))
            }))
            .when(field.is_none(), |d| d.text_color(muted))
    }

    /// Row of choice chips for a small enum. Incompatible options are
    /// disabled (dashed, lock icon) with the reason as tooltip; hover feeds
    /// the context panel.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn chips<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        options: &[T],
        selected: T,
        label: fn(T) -> &'static str,
        reason: impl Fn(T) -> Option<&'static str>,
        recommended: Option<T>,
        topic: Option<fn(T) -> Topic>,
        on_pick: fn(&mut kw_core::ClusterSpec, T),
        cx: &mut Context<Self>,
    ) -> Div {
        let p = pal(cx);
        let rec_label = i18n::t(Self::lang(cx), K::Recommended);
        h_flex().flex_wrap().gap_2().children(options.iter().enumerate().map(|(i, &opt)| {
            let why = reason(opt);
            let sel = opt == selected;
            let rec = recommended == Some(opt) && why.is_none();
            let mut chip = div()
                .id((id, i))
                .flex()
                .items_center()
                .gap_1p5()
                .px_3()
                .py_2()
                .rounded(px(10.))
                .border_1()
                .text_sm()
                .font_semibold()
                .bg(if sel { p.prs } else { p.sf })
                .border_color(if sel { p.pr } else { p.bd })
                .when(sel, |d| d.child(Icon::new(IconName::CircleCheck).xsmall().text_color(p.prt)))
                .child(label(opt))
                .when(rec, |d| d.child(pill(p.oks, p.okt, rec_label)));
            if let Some(why) = why {
                chip = chip
                    .border_dashed()
                    .opacity(0.6)
                    .cursor_not_allowed()
                    .child(Icon::new(Lucide::Lock).xsmall().text_color(p.mu))
                    .tooltip(move |window, cx| Tooltip::new(why).build(window, cx));
            } else {
                chip = chip
                    .cursor_pointer()
                    .hover(move |st| st.border_color(p.pr))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        let before = this.spec.target.clone();
                        this.edit(|s| on_pick(s, opt), cx);
                        if this.spec.target != before {
                            this.sync_inputs_where(|i| i.is_tofu(), window, cx);
                        }
                    }));
            }
            if let Some(tf) = topic {
                chip = chip.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    let t = tf(opt);
                    if *hovered && this.topic != Some(t) {
                        this.topic = Some(t);
                        cx.notify();
                    }
                }));
            }
            chip
        }))
    }

    /// Status shown under a stepper item: icon, text key, color.
    fn step_status(&self, s: Step, p: &Palette) -> (Icon, K, Hsla) {
        let visited = self.visited & (1 << s.index()) != 0;
        let sev = self.report.step_severity(s);
        if !self.reachable(s) {
            return (Icon::new(Lucide::Lock), K::StLocked, p.mu);
        }
        match (sev, s == self.step, visited) {
            (Some(Severity::Error), _, _) => (Icon::new(IconName::CircleX), K::StError, p.ert),
            (_, true, _) => (Icon::new(Lucide::Sparkles), K::StCurrent, p.prt),
            (Some(Severity::Warning), _, _) => (Icon::new(IconName::TriangleAlert), K::StWarning, p.wnt),
            (None, _, true) => (Icon::new(IconName::CircleCheck), K::StDone, p.okt),
            (None, _, false) => (Icon::new(Lucide::Circle), K::StTodo, p.mu),
        }
    }

    fn render_stepper(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let p = pal(cx);
        let white = gpui_kit::white();
        let list = v_flex()
            .id("stepper")
            .w(px(200.))
            .flex_none()
            .h_full()
            .px_2()
            .py(px(10.))
            .gap(px(2.))
            .bg(p.sf)
            .border_r_1()
            .border_color(p.bd)
            .overflow_y_scroll()
            .children(Step::ALL.iter().map(|&s| {
                let cur = s == self.step;
                let locked = !self.reachable(s);
                let (st_icon, st_key, col) = self.step_status(s, &p);
                let item = h_flex()
                    .id(("step", s.index()))
                    .gap(px(9.))
                    .items_center()
                    .px_2()
                    .py(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if cur { p.pr } else { gpui_kit::transparent_black() })
                    .when(cur, |d| d.bg(p.prs))
                    .text_color(if locked { p.mu } else { p.tx })
                    .child(
                        div()
                            .size(px(28.))
                            .flex_none()
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if cur { p.pr } else { p.s2 })
                            .text_color(if cur { white } else { col })
                            .child(step_icon(s).small()),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(div().text_sm().font_semibold().child(format!("{}. {}", s.index() + 1, i18n::step_title(lang, s))))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .text_xs()
                                    .text_color(col)
                                    .child(st_icon.xsmall())
                                    .child(i18n::t(lang, st_key)),
                            ),
                    );
                if locked {
                    item.opacity(0.7)
                        .cursor_not_allowed()
                        .tooltip(move |window, cx| Tooltip::new(i18n::t(lang, K::StepLocked)).build(window, cx))
                } else {
                    item.cursor_pointer()
                        .when(!cur, |d| d.hover(move |st| st.bg(p.s2)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.go(s, cx)))
                }
            }));
        tour_ring(list, "stepper", cx)
    }

    fn render_context(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let topic = self.topic.unwrap_or(Topic::Step(self.step));
        let info = help::topic(topic);
        let p = pal(cx);
        let mono = cx.theme().mono_font_family.clone();
        let s = &self.spec;
        let (cp_fp, w_fp) = kw_core::resources::estimate(s);
        let row = |k: &'static str, v: SharedString| {
            h_flex()
                .justify_between()
                .gap_2()
                .px_3()
                .py(px(7.))
                .border_t_1()
                .border_color(p.bd)
                .text_xs()
                .child(div().text_color(p.mu).child(k))
                .child(div().font_family(mono.clone()).child(v))
        };
        let card = |title: &'static str| {
            v_flex()
                .rounded(px(10.))
                .border_1()
                .border_color(p.bd)
                .bg(p.sf)
                .overflow_hidden()
                .child(div().px_3().py_2().text_xs().font_bold().child(title))
        };
        let panel = v_flex()
            .id("context-panel")
            .w(px(280.))
            .flex_none()
            .h_full()
            .p(px(14.))
            .gap_3()
            .bg(p.sf)
            .border_l_1()
            .border_color(p.bd)
            .overflow_y_scroll()
            .child(div().text_xs().font_bold().text_color(p.mu).child(i18n::t(lang, K::Context)))
            .child(
                v_flex()
                    .gap_2()
                    .child(div().font_bold().child(info.title))
                    .child(div().text_sm().text_color(p.mu).child(info.body))
                    .child(
                        Button::new("ctx-doc")
                            .link()
                            .small()
                            .icon(Lucide::ExternalLink)
                            .label(i18n::t(lang, K::Learn))
                            .on_click(cx.listener(move |_, _, _, cx| cx.emit(WizardEvent::OpenDocs(info.doc)))),
                    ),
            )
            .child(
                card(i18n::t(lang, K::Summary))
                    .child(row("Distro", format!("{} {}", s.distro.id.label(), s.distro.version).into()))
                    .child(row("Nodes", format!("{} cp + {} w", s.topology.control_planes, s.topology.workers).into()))
                    .child(row("HA", (if s.topology.ha { "Yes" } else { "HA-ready" }).into()))
                    .child(row("CNI", s.network.cni.label().into()))
                    .child(row("Endpoint", s.endpoint.mode.label().into()))
                    .child(row("Service LB", s.service_lb.kind.label().into()))
                    .child(row("Ingress", s.ingress.label().into()))
                    .child(row("GitOps", s.gitops.tool.label().into()))
                    .child(row("Add-ons", s.addons.len().to_string().into())),
            )
            .child(
                card(i18n::t(lang, K::Footprint))
                    .child(row("Control plane", format!("{} MiB · {}m CPU", cp_fp.mem_mib, cp_fp.cpu_m).into()))
                    .child(row("Worker", format!("{} MiB · {}m CPU", w_fp.mem_mib, w_fp.cpu_m).into())),
            );
        tour_ring(panel, "context-panel", cx)
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let prev = self.step_offset(-1);
        let next = self.step_offset(1);
        let blocked = self.step_blocked();
        let p = pal(cx);
        let footer = h_flex()
            .id("nav-footer")
            .justify_between()
            .items_center()
            .px(px(18.))
            .py(px(12.))
            .bg(p.sf)
            .border_t_1()
            .border_color(p.bd)
            .child(
                Button::new("back")
                    .outline()
                    .icon(IconName::ArrowLeft)
                    .label(i18n::t(lang, K::Back))
                    .on_click(cx.listener(move |this, _, _, cx| match prev {
                        Some(s) => this.go(s, cx),
                        None => cx.emit(WizardEvent::GoHome),
                    })),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .when(blocked, |f| {
                        f.child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .text_sm()
                                .text_color(p.ert)
                                .child(Icon::new(IconName::CircleX).small())
                                .child(i18n::t(lang, K::FixErrors)),
                        )
                    })
                    .when_some(next, |f, n| {
                        f.child(
                            Button::new("next")
                                .primary()
                                .icon(IconName::ArrowRight)
                                .label(format!("{} · {}", i18n::t(lang, K::Next), i18n::step_title(lang, n)))
                                .disabled(blocked)
                                .when(blocked, |b| b.tooltip(i18n::t(lang, K::FixErrors)))
                                .on_click(cx.listener(move |this, _, _, cx| this.go(n, cx))),
                        )
                    }),
            );
        tour_ring(footer, "nav-footer", cx)
    }
}

/// Rounded status/badge pill (soft background + strong text).
pub(crate) fn pill(bg: Hsla, fg: Hsla, text: impl Into<SharedString>) -> Div {
    div()
        .flex_none()
        .px(px(7.))
        .py(px(1.))
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_xs()
        .font_bold()
        .child(text.into())
}

impl Render for Wizard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let p = pal(cx);
        let content = self.render_step(window, cx);
        let content_frame = v_flex()
            .id("content")
            .flex_1()
            .min_w_0()
            .h_full()
            .px(px(28.))
            .py(px(22.))
            .gap(px(18.))
            .bg(p.bg)
            .overflow_y_scroll()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .size(px(40.))
                            .flex_none()
                            .rounded(px(10.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(p.prs)
                            .text_color(p.prt)
                            .child(step_icon(self.step).size(px(22.))),
                    )
                    .child(
                        v_flex()
                            .child(div().text_xl().font_bold().child(i18n::step_title(lang, self.step)))
                            .child(div().text_sm().text_color(p.mu).child(i18n::step_desc(lang, self.step))),
                    ),
            )
            .child(content);
        h_flex()
            .size_full()
            .bg(p.bg)
            .child(self.render_stepper(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(tour_ring(content_frame, "content", cx))
                    .child(self.render_footer(cx)),
            )
            .child(self.render_context(cx))
    }
}

/// Icon shown next to a step in the stepper and title.
pub(crate) fn step_icon(step: Step) -> Icon {
    Icon::new(match step {
        Step::Project => Lucide::FolderOpen,
        Step::Target => Lucide::Cloud,
        Step::Distro => Lucide::Boxes,
        Step::Topology => Lucide::Server,
        Step::Endpoint => Lucide::Waypoints,
        Step::Network => Lucide::Network,
        Step::Ingress => Lucide::Route,
        Step::Features => Lucide::Puzzle,
        Step::GitOps => Lucide::GitBranch,
        Step::Tuning => Lucide::Gauge,
        Step::Review => Lucide::ClipboardCheck,
        Step::Deploy => Lucide::Rocket,
    })
}
