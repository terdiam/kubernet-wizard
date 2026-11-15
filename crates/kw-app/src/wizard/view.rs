//! Wizard shell: stepper, content frame, context panel, footer + widgets.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_core::{Field, Severity, Step};

use super::{Wizard, WizardEvent};
use crate::docs::DocPage;
use crate::help::{self, Topic};
use crate::i18n::{self, K, Lang};
use crate::settings::Ui;

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

    /// Row of choice buttons for a small enum. Incompatible options are
    /// disabled with the reason as tooltip; hover feeds the context panel.
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
        let rec_label = i18n::t(Self::lang(cx), K::Recommended);
        h_flex().flex_wrap().gap_2().children(options.iter().enumerate().map(|(i, &opt)| {
            let why = reason(opt);
            let is_rec = recommended == Some(opt);
            let mut b = Button::new((id, i))
                .small()
                .label(label(opt))
                .selected(opt == selected)
                .disabled(why.is_some())
                .when(opt == selected, |b| b.primary())
                .when(opt != selected, |b| b.outline())
                .on_click(cx.listener(move |this, _, window, cx| {
                    let before = this.spec.target.clone();
                    this.edit(|s| on_pick(s, opt), cx);
                    if this.spec.target != before {
                        this.sync_inputs_where(|i| i.is_tofu(), window, cx);
                    }
                }));
            if let Some(why) = why {
                b = b.tooltip(why);
            } else if is_rec {
                b = b.tooltip(rec_label).icon(IconName::Check);
            }
            if let Some(tf) = topic {
                b = b.on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    let t = tf(opt);
                    if *hovered && this.topic != Some(t) {
                        this.topic = Some(t);
                        cx.notify();
                    }
                }));
            }
            b
        }))
    }

    fn render_stepper(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let theme = cx.theme();
        let (danger, warning, success, muted, border) =
            (theme.danger, theme.warning, theme.success, theme.muted_foreground, theme.border);
        let list = v_flex().id("stepper").w(px(200.)).h_full().p_2().gap_1().border_r_1().border_color(border).children(
            Step::ALL.iter().map(|&s| {
                let visited = self.visited & (1 << s.index()) != 0;
                let (icon, color) = match (self.report.step_severity(s), visited) {
                    (Some(Severity::Error), _) => (IconName::CircleX, danger),
                    (Some(Severity::Warning), _) => (IconName::TriangleAlert, warning),
                    (None, true) => (IconName::CircleCheck, success),
                    (None, false) => (IconName::ChevronRight, muted),
                };
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(icon).small().text_color(color))
                    .child(
                        Button::new(("step", s.index()))
                            .ghost()
                            .small()
                            .flex_1()
                            .label(format!("{}. {}", s.index() + 1, i18n::step_title(lang, s)))
                            .selected(s == self.step)
                            .on_click(cx.listener(move |this, _, _, cx| this.go(s, cx))),
                    )
            }),
        );
        tour_ring(list, "stepper", cx)
    }

    fn render_context(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let topic = self.topic.unwrap_or(Topic::Step(self.step));
        let info = help::topic(topic);
        let theme = cx.theme();
        let (border, muted, card) = (theme.border, theme.muted_foreground, theme.secondary);
        let s = &self.spec;
        let (cp_fp, w_fp) = kw_core::resources::estimate(s);
        let row = |k: &'static str, v: SharedString| {
            h_flex().justify_between().text_xs().child(div().text_color(muted).child(k)).child(v)
        };
        let panel = v_flex()
            .id("context-panel")
            .w(px(280.))
            .h_full()
            .p_3()
            .gap_3()
            .border_l_1()
            .border_color(border)
            .overflow_y_scroll()
            .child(
                v_flex()
                    .gap_2()
                    .child(div().font_semibold().child(info.title))
                    .child(div().text_sm().child(info.body))
                    .child(
                        Button::new("ctx-doc")
                            .link()
                            .small()
                            .icon(IconName::BookOpen)
                            .label(i18n::t(lang, K::Learn))
                            .on_click(cx.listener(move |_, _, _, cx| cx.emit(WizardEvent::OpenDocs(info.doc)))),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(card)
                    .child(div().text_sm().font_semibold().child(i18n::t(lang, K::Summary)))
                    .child(row("Distro", format!("{} {}", s.distro.id.label(), s.distro.version).into()))
                    .child(row(
                        "Control planes",
                        format!("{}{}", s.topology.control_planes, if s.topology.ha { " (HA)" } else { " (HA-ready)" }).into(),
                    ))
                    .child(row("Workers", s.topology.workers.to_string().into()))
                    .child(row("CNI", s.network.cni.label().into()))
                    .child(row("Endpoint", s.endpoint.mode.label().into()))
                    .child(row("Service LB", s.service_lb.kind.label().into()))
                    .child(row("Ingress", s.ingress.label().into()))
                    .child(row("GitOps", s.gitops.tool.label().into()))
                    .child(row("Add-ons", s.addons.len().to_string().into())),
            )
            .child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(card)
                    .child(div().text_sm().font_semibold().child(i18n::t(lang, K::Footprint)))
                    .child(row("Control plane", format!("~{} MiB · {}m CPU", cp_fp.mem_mib, cp_fp.cpu_m).into()))
                    .child(row("Worker", format!("~{} MiB · {}m CPU", w_fp.mem_mib, w_fp.cpu_m).into())),
            );
        tour_ring(panel, "context-panel", cx)
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let prev = self.step_offset(-1);
        let next = self.step_offset(1);
        let blocked = self.step_blocked();
        let border = cx.theme().border;
        let danger = cx.theme().danger;
        let footer = h_flex()
            .id("nav-footer")
            .justify_between()
            .items_center()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(border)
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
            .when(blocked, |f| f.child(div().text_sm().text_color(danger).child(i18n::t(lang, K::FixErrors))))
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
            });
        tour_ring(footer, "nav-footer", cx)
    }
}

impl Render for Wizard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = Self::lang(cx);
        let content = self.render_step(window, cx);
        let content_frame = v_flex()
            .id("content")
            .flex_1()
            .min_w_0()
            .h_full()
            .p_5()
            .gap_4()
            .overflow_y_scroll()
            .child(div().text_xl().font_semibold().child(i18n::step_title(lang, self.step)))
            .child(content);
        h_flex()
            .size_full()
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
