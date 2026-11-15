//! App shell: header, screens (Welcome, Home, Wizard, Docs), tour card.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_core::ClusterSpec;
use kw_core::presets::Preset;
use kw_exec::prereq::{self, ToolState, ToolStatus};

use crate::assets;
use crate::docs::DocsView;
use crate::i18n::{self, K};
use crate::onboarding::{MAIN_TOUR, Screen};
use crate::settings::{self, Ui};
use crate::theme::pal;
use crate::wizard::{Wizard, WizardEvent};

pub struct KwApp {
    pub(crate) update: crate::update::UpdateState,
    pub(crate) pending_notice: Option<SharedString>,
    screen: Screen,
    welcome: bool,
    pub(crate) wizard: Entity<Wizard>,
    docs: Entity<DocsView>,
    prereq: Option<Vec<ToolStatus>>,
    pending_load: Option<(ClusterSpec, std::path::PathBuf)>,
    pending_error: Option<SharedString>,
    _subs: [Subscription; 1],
}

impl KwApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let welcome = !cx.global::<Ui>().prefs.welcomed;
        let wizard = cx.new(|cx| Wizard::new(Preset::DevLocal.build(), window, cx));
        let docs = cx.new(|cx| DocsView::new(window, cx));
        let sub = cx.subscribe(&wizard, |this: &mut Self, _, ev: &WizardEvent, cx| match ev {
            WizardEvent::OpenDocs(page) => {
                let page = *page;
                this.docs.update(cx, |d, cx| d.open(page, cx));
                this.screen = Screen::Docs;
                cx.notify();
            }
            WizardEvent::GoHome => {
                this.screen = Screen::Home;
                cx.notify();
            }
        });
        let pending_notice = cx.global_mut::<Ui>().startup_notice.take();
        let mut this = Self {
            update: crate::update::UpdateState::Idle,
            pending_notice,
            screen: Screen::Home,
            welcome,
            wizard,
            docs,
            prereq: None,
            pending_load: None,
            pending_error: None,
            _subs: [sub],
        };
        this.auto_update_check(cx);
        // Confirm a fresh update once the UI has run for a while (rollback guard).
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(10)).await;
            kw_update::mark_healthy(&settings::state_dir());
        })
        .detach();
        if welcome {
            this.check_prereq(cx);
        }
        this
    }

    fn check_prereq(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { prereq::check_all(true) }).await;
            let _ = this.update(cx, |a, cx| {
                a.prereq = Some(res);
                cx.notify();
            });
        })
        .detach();
    }

    fn update_prefs(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut settings::Prefs)) {
        let ui = cx.global_mut::<Ui>();
        f(&mut ui.prefs);
        settings::save(&ui.prefs);
        self.refresh_children(cx);
    }

    /// Children read the global during render; ask them to redraw.
    fn refresh_children(&mut self, cx: &mut Context<Self>) {
        self.wizard.update(cx, |_, cx| cx.notify());
        self.docs.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    fn set_tour(&mut self, step: Option<usize>, cx: &mut Context<Self>) {
        cx.global_mut::<Ui>().tour = step;
        if let Some(s) = step.and_then(|i| MAIN_TOUR.get(i)) {
            self.screen = s.screen;
        }
        self.refresh_children(cx);
    }

    fn finish_welcome(&mut self, tour: bool, cx: &mut Context<Self>) {
        self.welcome = false;
        self.update_prefs(cx, |p| p.welcomed = true);
        if tour {
            self.set_tour(Some(0), cx);
        }
    }

    fn start_with(&mut self, spec: ClusterSpec, dir: Option<std::path::PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.wizard.update(cx, |w, cx| w.load(spec, dir, window, cx));
        self.screen = Screen::Wizard;
        cx.notify();
    }

    fn open_project(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open kubernet-wizard project".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(dir) = paths.into_iter().next() else { return };
            let loaded = std::fs::read_to_string(dir.join("cluster.yaml"))
                .map_err(|e| e.to_string())
                .and_then(|t| ClusterSpec::from_yaml(&t).map_err(|e| e.to_string()));
            let _ = this.update(cx, |app, cx| {
                // Window access is needed to sync inputs; defer to next frame.
                match loaded {
                    Ok(spec) => app.pending_load = Some((spec, dir)),
                    Err(e) => app.pending_error = Some(format!("cluster.yaml: {e}").into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

// Deferred results from async tasks that need `Window` to apply.
impl KwApp {
    fn apply_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((spec, dir)) = self.pending_load.take() {
            self.start_with(spec, Some(dir), window, cx);
        }
        if let Some(msg) = self.pending_error.take() {
            window.push_notification(msg, cx);
        }
        if let Some(msg) = self.pending_notice.take() {
            window.push_notification(msg, cx);
        }
    }
}

impl KwApp {
    /// Header nav pill (selected = soft primary).
    fn nav_pill(&self, id: &'static str, label: &'static str, target: Screen, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = pal(cx);
        let on = self.screen == target;
        div()
            .id(id)
            .px_3()
            .py(px(6.))
            .rounded(px(7.))
            .text_sm()
            .font_semibold()
            .cursor_pointer()
            .bg(if on { p.prs } else { transparent_black() })
            .text_color(if on { p.prt } else { p.mu })
            .when(!on, |d| d.hover(move |st| st.bg(p.s2)))
            .child(label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.screen = target;
                cx.notify();
            }))
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Ui>();
        let (lang, advanced, tour_mode) = (ui.prefs.lang, ui.prefs.advanced, ui.tour_target() == Some("mode-toggle"));
        let p = pal(cx);
        let wiz = self.wizard.read(cx);
        let (can_undo, can_redo) = (wiz.can_undo(), wiz.can_redo());
        let in_wizard = self.screen == Screen::Wizard;
        let seg = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .when(on, |d| d.bg(p.sf).shadow_xs())
                .text_color(if on { p.tx } else { p.mu })
                .child(label)
        };
        h_flex()
            .h(px(52.))
            .flex_none()
            .items_center()
            .gap(px(14.))
            .px(px(14.))
            .bg(p.sf)
            .border_b_1()
            .border_color(p.bd)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .font_bold()
                    .child(img(assets::LOGO).size(px(28.)))
                    .child("kubernet-wizard"),
            )
            .child(
                h_flex()
                    .gap(px(2.))
                    .ml_2()
                    .child(self.nav_pill("nav-home", i18n::t(lang, K::Home), Screen::Home, cx))
                    .child(self.nav_pill("nav-wizard", "Wizard", Screen::Wizard, cx))
                    .child(self.nav_pill("nav-docs", i18n::t(lang, K::Docs), Screen::Docs, cx)),
            )
            .child(div().flex_1())
            .child(
                h_flex()
                    .id("mode-toggle")
                    .p(px(2.))
                    .rounded(px(8.))
                    .bg(p.s2)
                    .text_xs()
                    .font_semibold()
                    .when(tour_mode, |d| d.border_2().border_color(p.pr))
                    .child(
                        seg("mode-basic", i18n::t(lang, K::Basic), !advanced)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.update_prefs(cx, |p| p.advanced = false))),
                    )
                    .child(
                        seg("mode-adv", i18n::t(lang, K::Advanced), advanced)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.update_prefs(cx, |p| p.advanced = true))),
                    ),
            )
            .when(in_wizard, |r| {
                r.child(
                    h_flex()
                        .gap(px(2.))
                        .child(
                            Button::new("undo")
                                .ghost()
                                .small()
                                .icon(IconName::Undo2)
                                .tooltip("Undo")
                                .disabled(!can_undo)
                                .on_click(cx.listener(|this, _, window, cx| this.wizard.update(cx, |w, cx| w.undo(window, cx)))),
                        )
                        .child(
                            Button::new("redo")
                                .ghost()
                                .small()
                                .icon(IconName::Redo2)
                                .tooltip("Redo")
                                .disabled(!can_redo)
                                .on_click(cx.listener(|this, _, window, cx| this.wizard.update(cx, |w, cx| w.redo(window, cx)))),
                        ),
                )
            })
            .child(
                Button::new("lang")
                    .outline()
                    .small()
                    .icon(IconName::Globe)
                    .label(lang.code())
                    .tooltip("Bahasa / Language")
                    .on_click(cx.listener(|this, _, _, cx| this.update_prefs(cx, |p| p.lang = p.lang.toggle()))),
            )
            .children(self.update_button(cx))
            .child(
                Button::new("tour")
                    .ghost()
                    .small()
                    .icon(IconName::Map)
                    .tooltip(i18n::t(lang, K::Tour))
                    .on_click(cx.listener(|this, _, _, cx| this.set_tour(Some(0), cx))),
            )
            .child(
                Button::new("settings")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .tooltip(i18n::t(lang, K::Settings))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings_dialog(window, cx))),
            )
    }

    fn home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Ui>();
        let (lang, tour_presets) = (ui.prefs.lang, ui.tour_target() == Some("home-presets"));
        let p = pal(cx);
        let mono = cx.theme().mono_font_family.clone();
        let action = |id: &'static str, icon: Icon, title: &'static str, hint: &'static str| {
            v_flex()
                .id(id)
                .flex_1()
                .gap_1()
                .p(px(18.))
                .rounded(px(12.))
                .border_1()
                .border_color(p.bd)
                .bg(p.sf)
                .cursor_pointer()
                .hover(move |st| st.border_color(p.pr))
                .child(div().text_color(p.prt).child(icon.size(px(26.))))
                .child(div().mt_2().font_bold().child(title))
                .child(div().text_sm().text_color(p.mu).child(hint))
        };
        let presets = div()
            .id("home-presets")
            .grid()
            .grid_cols(4)
            .gap_3()
            .rounded(px(12.))
            .when(tour_presets, |d| d.border_2().border_color(p.pr))
            .children(Preset::ALL.iter().enumerate().map(|(i, &pr)| {
                let spec = pr.build();
                let nodes = u16::from(spec.topology.control_planes) + u16::from(spec.topology.workers);
                v_flex()
                    .id(("preset", i))
                    .gap_2()
                    .p(px(14.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(p.bd)
                    .bg(p.sf)
                    .cursor_pointer()
                    .hover(move |st| st.border_color(p.pr))
                    .child(
                        div()
                            .size(px(36.))
                            .rounded(px(9.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(p.prs)
                            .text_color(p.prt)
                            .child(preset_icon(pr)),
                    )
                    .child(div().font_bold().child(pr.title()))
                    .child(div().text_xs().text_color(p.mu).font_family(mono.clone()).child(pr.summary()))
                    .child(
                        h_flex()
                            .mt_auto()
                            .gap_1()
                            .items_center()
                            .text_xs()
                            .text_color(p.mu)
                            .child(Icon::new(Lucide::Server).xsmall())
                            .child(format!("{nodes} {} · {}", if nodes == 1 { "node" } else { "nodes" }, spec.distro.id.label())),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_with(pr.build(), None, window, cx)))
            }));

        v_flex()
            .id("home")
            .size_full()
            .px(px(48.))
            .py(px(30.))
            .gap(px(22.))
            .bg(p.bg)
            .overflow_y_scroll()
            .child(
                h_flex()
                    .gap(px(18.))
                    .items_center()
                    .child(img(assets::LOGO).size(px(64.)))
                    .child(
                        v_flex()
                            .child(div().text_2xl().font_bold().child(i18n::t(lang, K::HomeTitle)))
                            .child(div().text_color(p.mu).child(i18n::t(lang, K::Tagline))),
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        action("home-new", Icon::new(IconName::Plus), i18n::t(lang, K::NewCluster), i18n::t(lang, K::NewClusterHint))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_with(Preset::DevLocal.build(), None, window, cx))),
                    )
                    .child(
                        action("home-open", Icon::new(IconName::FolderOpen), i18n::t(lang, K::OpenProject), i18n::t(lang, K::OpenProjectHint))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_project(cx))),
                    )
                    .child(
                        action("home-docs", Icon::new(IconName::BookOpen), i18n::t(lang, K::Docs), i18n::t(lang, K::DocsHint)).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.screen = Screen::Docs;
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(div().text_sm().font_bold().child(i18n::t(lang, K::Presets)))
            .child(presets)
    }

    fn welcome_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = cx.global::<Ui>().prefs.lang;
        let p = pal(cx);
        let mono = cx.theme().mono_font_family.clone();
        let list = self.prereq.as_deref().unwrap_or_default();
        let ready = list.iter().filter(|s| s.is_ok()).count();
        v_flex()
            .id("welcome")
            .size_full()
            .items_center()
            .px_6()
            .py(px(36.))
            .gap(px(18.))
            .bg(p.bg)
            .overflow_y_scroll()
            .child(img(assets::LOGO).size(px(88.)))
            .child(div().text_3xl().font_bold().child(i18n::t(lang, K::WelcomeTitle)))
            .child(div().max_w(px(560.)).text_center().text_color(p.mu).child(i18n::t(lang, K::WelcomeBody)))
            .child(
                v_flex()
                    .w(px(640.))
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
                            .py(px(14.))
                            .child(div().text_sm().font_bold().child(i18n::t(lang, K::Prereq)))
                            .when(!list.is_empty(), |d| {
                                d.child(div().text_xs().text_color(p.mu).child(format!("{ready} / {}", list.len())))
                            }),
                    )
                    .when(self.prereq.is_none(), |d| {
                        d.child(div().px(px(18.)).py(px(10.)).border_t_1().border_color(p.bd).text_sm().text_color(p.mu).child("…"))
                    })
                    .children(list.iter().map(|s| {
                        let (icon, bg, fg, label, ver) = match s.state {
                            ToolState::Ok { version } => (IconName::CircleCheck, p.oks, p.okt, K::StDone, format!("{}.{}", version.0, version.1)),
                            ToolState::Outdated { version } => {
                                (IconName::TriangleAlert, p.wns, p.wnt, K::StWarning, format!("{}.{}", version.0, version.1))
                            }
                            ToolState::Missing if s.tool.cloud_only => (IconName::TriangleAlert, p.wns, p.wnt, K::StWarning, "—".to_owned()),
                            ToolState::Missing => (IconName::CircleX, p.ers, p.ert, K::StError, "—".to_owned()),
                        };
                        h_flex()
                            .gap_3()
                            .items_center()
                            .px(px(18.))
                            .py(px(10.))
                            .border_t_1()
                            .border_color(p.bd)
                            .text_sm()
                            .child(div().w(px(120.)).font_semibold().child(s.tool.name))
                            .child(div().w(px(70.)).text_xs().text_color(p.mu).font_family(mono.clone()).child(ver))
                            .child(
                                h_flex()
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
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .text_color(p.mu)
                                    .text_right()
                                    .when(!s.is_ok(), |d| d.child(s.tool.install.current())),
                            )
                    })),
            )
            .child(
                h_flex()
                    .gap(px(10.))
                    .mt_1()
                    .child(
                        Button::new("w-tour")
                            .primary()
                            .icon(IconName::Map)
                            .label(i18n::t(lang, K::StartTour))
                            .on_click(cx.listener(|this, _, _, cx| this.finish_welcome(true, cx))),
                    )
                    .child(
                        Button::new("w-skip")
                            .ghost()
                            .label(i18n::t(lang, K::Skip))
                            .on_click(cx.listener(|this, _, _, cx| this.finish_welcome(false, cx))),
                    ),
            )
    }

    fn tour_card(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let ui = cx.global::<Ui>();
        let idx = ui.tour?;
        let lang = ui.prefs.lang;
        let step = MAIN_TOUR.get(idx)?;
        let last = idx + 1 == MAIN_TOUR.len();
        let theme = cx.theme();
        let (bg, border, muted) = (theme.popover, theme.ring, theme.muted_foreground);
        Some(
            div().absolute().bottom(px(64.)).right(px(24.)).child(
                v_flex()
                    .w(px(340.))
                    .p_4()
                    .gap_2()
                    .rounded_lg()
                    .border_2()
                    .border_color(border)
                    .bg(bg)
                    .shadow_lg()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(div().font_semibold().child(step.title(lang)))
                            .child(div().text_xs().text_color(muted).child(format!("{}/{}", idx + 1, MAIN_TOUR.len()))),
                    )
                    .child(div().text_sm().child(step.body(lang)))
                    .child(
                        h_flex()
                            .justify_between()
                            .child(
                                Button::new("tour-skip")
                                    .ghost()
                                    .small()
                                    .label(i18n::t(lang, K::Skip))
                                    .on_click(cx.listener(|this, _, _, cx| this.set_tour(None, cx))),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(
                                        Button::new("tour-back")
                                            .outline()
                                            .small()
                                            .icon(IconName::ArrowLeft)
                                            .disabled(idx == 0)
                                            .on_click(cx.listener(move |this, _, _, cx| this.set_tour(idx.checked_sub(1), cx))),
                                    )
                                    .child(
                                        Button::new("tour-next")
                                            .primary()
                                            .small()
                                            .label(if last { i18n::t(lang, K::TourDone) } else { i18n::t(lang, K::Next) })
                                            .on_click(cx.listener(move |this, _, _, cx| this.set_tour((!last).then_some(idx + 1), cx))),
                                    ),
                            ),
                    ),
            ),
        )
    }
}

impl Render for KwApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending(window, cx);
        let p = pal(cx);
        let (bg, fg) = (p.bg, p.tx);
        let body: AnyElement = if self.welcome {
            self.welcome_screen(cx).into_any_element()
        } else {
            match self.screen {
                Screen::Home => self.home(cx).into_any_element(),
                Screen::Wizard => self.wizard.clone().into_any_element(),
                Screen::Docs => self.docs.clone().into_any_element(),
            }
        };
        let header = if self.welcome { None } else { Some(self.header(cx).into_any_element()) };
        let tour = self.tour_card(cx).map(IntoElement::into_any_element);
        v_flex()
            .size_full()
            .relative()
            .bg(bg)
            .text_color(fg)
            .children(header)
            .child(div().flex_1().min_h_0().child(body))
            .children(tour)
    }
}

fn preset_icon(p: Preset) -> Icon {
    Icon::new(match p {
        Preset::DevLocal => Lucide::Laptop,
        Preset::Homelab => Lucide::House,
        Preset::ProductionOnPrem => Lucide::Building2,
        Preset::CloudHetzner => Lucide::Cloud,
    })
}
