//! App shell: header, screens (Welcome, Home, Wizard, Docs), tour card.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_core::ClusterSpec;
use kw_core::presets::Preset;
use kw_exec::prereq::{self, ToolState, ToolStatus};

use crate::docs::DocsView;
use crate::i18n::{self, K};
use crate::onboarding::{MAIN_TOUR, Screen};
use crate::settings::{self, Ui};
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
    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Ui>();
        let (lang, advanced, tour_mode) = (ui.prefs.lang, ui.prefs.advanced, ui.tour_target() == Some("mode-toggle"));
        let theme = cx.theme();
        let (border, ring) = (theme.border, theme.ring);
        let wiz = self.wizard.read(cx);
        let (can_undo, can_redo) = (wiz.can_undo(), wiz.can_redo());
        let in_wizard = self.screen == Screen::Wizard;
        h_flex()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::LayoutDashboard))
                    .child(div().font_semibold().child("kubernet-wizard"))
                    .child(
                        Button::new("nav-home")
                            .ghost()
                            .small()
                            .label(i18n::t(lang, K::Home))
                            .selected(self.screen == Screen::Home)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.screen = Screen::Home;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("nav-wizard")
                            .ghost()
                            .small()
                            .label("Wizard")
                            .selected(in_wizard)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.screen = Screen::Wizard;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("nav-docs")
                            .ghost()
                            .small()
                            .icon(IconName::BookOpen)
                            .label(i18n::t(lang, K::Docs))
                            .selected(self.screen == Screen::Docs)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.screen = Screen::Docs;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .when(in_wizard, |r| {
                        r.child(
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
                        )
                    })
                    .child(
                        h_flex()
                            .id("mode-toggle")
                            .gap_1()
                            .p_0p5()
                            .rounded_md()
                            .when(tour_mode, |d| d.border_2().border_color(ring))
                            .child(
                                Button::new("mode-basic")
                                    .small()
                                    .label(i18n::t(lang, K::Basic))
                                    .selected(!advanced)
                                    .when(!advanced, |b| b.primary())
                                    .when(advanced, |b| b.ghost())
                                    .on_click(cx.listener(|this, _, _, cx| this.update_prefs(cx, |p| p.advanced = false))),
                            )
                            .child(
                                Button::new("mode-adv")
                                    .small()
                                    .label(i18n::t(lang, K::Advanced))
                                    .selected(advanced)
                                    .when(advanced, |b| b.primary())
                                    .when(!advanced, |b| b.ghost())
                                    .on_click(cx.listener(|this, _, _, cx| this.update_prefs(cx, |p| p.advanced = true))),
                            ),
                    )
                    .child(
                        Button::new("tour")
                            .ghost()
                            .small()
                            .icon(IconName::Map)
                            .label(i18n::t(lang, K::Tour))
                            .on_click(cx.listener(|this, _, _, cx| this.set_tour(Some(0), cx))),
                    )
                    .children(self.update_button(cx))
                    .child(
                        Button::new("settings")
                            .ghost()
                            .small()
                            .icon(IconName::Settings)
                            .tooltip(i18n::t(lang, K::Settings))
                            .on_click(cx.listener(|this, _, window, cx| this.open_settings_dialog(window, cx))),
                    )
                    .child(
                        Button::new("lang")
                            .ghost()
                            .small()
                            .icon(IconName::Globe)
                            .label(lang.code())
                            .tooltip("Bahasa / Language")
                            .on_click(cx.listener(|this, _, _, cx| this.update_prefs(cx, |p| p.lang = p.lang.toggle()))),
                    ),
            )
    }

    fn home(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = cx.global::<Ui>();
        let (lang, tour_presets) = (ui.prefs.lang, ui.tour_target() == Some("home-presets"));
        let theme = cx.theme();
        let (border, muted, ring, card) = (theme.border, theme.muted_foreground, theme.ring, theme.secondary);
        let card_style = |icon: IconName, title: &'static str, hint: &'static str, button: Button| {
            v_flex()
                .flex_1()
                .p_4()
                .gap_2()
                .rounded_lg()
                .border_1()
                .border_color(border)
                .child(Icon::new(icon).large())
                .child(div().font_semibold().child(title))
                .child(div().text_sm().text_color(muted).child(hint))
                .child(button.small().primary().label(title))
        };
        let presets = div()
            .id("home-presets")
            .grid()
            .grid_cols(2)
            .gap_3()
            .p_1()
            .rounded_lg()
            .when(tour_presets, |d| d.border_2().border_color(ring))
            .children(Preset::ALL.iter().enumerate().map(|(i, &p)| {
                v_flex()
                    .p_3()
                    .gap_1()
                    .rounded_md()
                    .bg(card)
                    .child(div().font_semibold().child(p.title()))
                    .child(div().text_sm().text_color(muted).child(p.summary()))
                    .child(
                        Button::new(("preset", i))
                            .small()
                            .outline()
                            .icon(IconName::ArrowRight)
                            .label(i18n::t(lang, K::Presets))
                            .on_click(cx.listener(move |this, _, window, cx| this.start_with(p.build(), None, window, cx))),
                    )
            }));

        let new_card = card_style(
            IconName::Plus,
            i18n::t(lang, K::NewCluster),
            i18n::t(lang, K::NewClusterHint),
            Button::new("home-new").on_click(cx.listener(|this, _, window, cx| this.start_with(Preset::DevLocal.build(), None, window, cx))),
        );
        let open_card = card_style(
            IconName::FolderOpen,
            i18n::t(lang, K::OpenProject),
            i18n::t(lang, K::OpenProjectHint),
            Button::new("home-open").on_click(cx.listener(|this, _, _, cx| this.open_project(cx))),
        );
        let docs_card = card_style(
            IconName::BookOpen,
            i18n::t(lang, K::Docs),
            i18n::t(lang, K::DocsHint),
            Button::new("home-docs").on_click(cx.listener(|this, _, _, cx| {
                this.screen = Screen::Docs;
                cx.notify();
            })),
        );

        v_flex()
            .id("home")
            .size_full()
            .p_8()
            .gap_6()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_2xl().font_semibold().child("kubernet-wizard"))
                    .child(div().text_color(muted).child(i18n::t(lang, K::Tagline))),
            )
            .child(h_flex().gap_4().child(new_card).child(open_card).child(docs_card))
            .child(div().font_semibold().child(i18n::t(lang, K::Presets)))
            .child(presets)
    }

    fn welcome_screen(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = cx.global::<Ui>().prefs.lang;
        let theme = cx.theme();
        let (muted, card, success, danger, warning) = (theme.muted_foreground, theme.secondary, theme.success, theme.danger, theme.warning);
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(620.))
                    .gap_4()
                    .child(div().text_2xl().font_semibold().child(i18n::t(lang, K::WelcomeTitle)))
                    .child(div().text_color(muted).child(i18n::t(lang, K::WelcomeBody)))
                    .child(
                        v_flex()
                            .gap_1()
                            .p_3()
                            .rounded_md()
                            .bg(card)
                            .child(div().font_semibold().child(i18n::t(lang, K::Prereq)))
                            .when(self.prereq.is_none(), |d| d.child(div().text_sm().text_color(muted).child("…")))
                            .children(self.prereq.iter().flatten().map(|s| {
                                let (icon, color, note) = match s.state {
                                    ToolState::Ok { .. } => (IconName::CircleCheck, success, ""),
                                    ToolState::Outdated { .. } => (IconName::TriangleAlert, warning, s.tool.install.current()),
                                    ToolState::Missing if s.tool.cloud_only => (IconName::TriangleAlert, warning, s.tool.install.current()),
                                    ToolState::Missing => (IconName::CircleX, danger, s.tool.install.current()),
                                };
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .text_sm()
                                    .child(Icon::new(icon).small().text_color(color))
                                    .child(div().w(px(90.)).child(s.tool.name))
                                    .child(div().text_xs().text_color(muted).child(note))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
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
        let theme = cx.theme();
        let (bg, fg) = (theme.background, theme.foreground);
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
