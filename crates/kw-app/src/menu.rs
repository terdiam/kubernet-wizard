//! Native menu bar: About, Check for Updates, Quit.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, Sizable, WindowExt, v_flex};
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;

use crate::app::KwApp;
use crate::assets;
use crate::i18n::{self, K};
use crate::settings::Ui;
use crate::theme::pal;

actions!(kubernet_wizard, [About, CheckUpdates, Quit]);

const REPO: &str = "https://github.com/terdiam/kubernet-wizard";

/// Installs the menu bar, the quit shortcut and the global handlers that
/// forward About / Check for Updates to the main window. Call once.
pub fn install(app: WeakEntity<KwApp>, window: AnyWindowHandle, cx: &mut App) {
    set_menus(cx);
    let quit_key = if cfg!(target_os = "macos") { "cmd-q" } else { "ctrl-q" };
    cx.bind_keys([KeyBinding::new(quit_key, Quit, None)]);
    cx.on_action(|_: &Quit, cx| cx.quit());

    // Menu actions arrive while the window is being updated; defer so the
    // window can be borrowed again.
    let weak = app.clone();
    cx.on_action(move |_: &About, cx| {
        let weak = weak.clone();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = weak.update(cx, |app, cx| app.open_about(window, cx));
            });
        });
    });
    cx.on_action(move |_: &CheckUpdates, cx| {
        let app = app.clone();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.check_updates(true, cx);
                    app.open_settings_dialog(window, cx);
                });
            });
        });
    });
}

/// (Re)builds the menu titles in the current language.
pub fn set_menus(cx: &App) {
    let lang = cx.global::<Ui>().prefs.lang;
    cx.set_menus([Menu {
        name: "kubernet-wizard".into(),
        items: vec![
            MenuItem::action(i18n::t(lang, K::MenuAbout), About),
            MenuItem::separator(),
            MenuItem::action(i18n::t(lang, K::MenuCheckUpdates), CheckUpdates),
            MenuItem::separator(),
            MenuItem::action(i18n::t(lang, K::MenuQuit), Quit),
        ],
        disabled: false,
    }]);
}

impl KwApp {
    pub(crate) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let lang = cx.global::<Ui>().prefs.lang;
            let p = pal(cx);
            let check = {
                let this = this.clone();
                Button::new("about-check")
                    .outline()
                    .small()
                    .icon(IconName::RefreshCw)
                    .label(i18n::t(lang, K::MenuCheckUpdates))
                    .on_click(move |_, window, cx| {
                        let _ = this.update(cx, |a, cx| {
                            a.check_updates(true, cx);
                            a.open_settings_dialog(window, cx);
                        });
                    })
            };
            dialog.title(i18n::t(lang, K::MenuAbout)).w(px(420.)).child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .child(img(assets::LOGO).size(px(72.)))
                    .child(div().text_xl().font_bold().child("kubernet-wizard"))
                    .child(div().text_sm().text_color(p.mu).child(format!("v{}", kw_update::current_version())))
                    .child(div().text_sm().text_center().child(i18n::t(lang, K::Tagline)))
                    .child(div().text_xs().text_color(p.mu).child("Apache License 2.0 · fonts: Hanken Grotesk, JetBrains Mono (OFL)"))
                    .child(
                        Button::new("about-repo")
                            .link()
                            .small()
                            .icon(IconName::Github)
                            .label("github.com/terdiam/kubernet-wizard")
                            .on_click(|_, _, cx| cx.open_url(REPO)),
                    )
                    .child(check),
            )
        });
    }
}
