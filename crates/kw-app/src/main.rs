//! kubernet-wizard desktop app.
#![recursion_limit = "512"]
// Release builds on Windows are GUI apps: no console window behind the UI.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// GPUI idioms: `use gpui_kit::*` prelude, builder closures, render helpers on views.
#![allow(
    clippy::wildcard_imports,
    clippy::unused_self,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::redundant_closure_for_method_calls
)]

mod app;
mod assets;
mod docs;
mod help;
mod i18n;
mod menu;
mod onboarding;
mod settings;
mod setup;
mod theme;
mod update;
mod wizard;

use gpui_kit::*;

fn main() {
    kw_exec::tools::init_default();
    // Rollback guard must run before any UI work.
    let version = kw_update::current_version().to_string();
    let startup = kw_update::on_startup(&settings::state_dir(), &version);
    if let kw_update::StartupOutcome::RolledBack { relaunch } = &startup {
        if let Err(e) = kw_update::relaunch(relaunch) {
            eprintln!("kubernet-wizard: rollback relaunch failed: {e}");
        }
        return;
    }
    update::install_cached_catalog();
    let prefs = settings::load();
    let startup_notice = match startup {
        kw_update::StartupOutcome::JustUpdated { version } => {
            Some(format!("{} v{version}", i18n::t(prefs.lang, i18n::K::UpdateInstalled)).into())
        }
        _ => None,
    };

    gpui_kit::application().with_assets(assets::KwAssets).run(|cx| {
        gpui_kit::init(cx);
        let fonts = theme::load_fonts(cx);
        cx.set_global(theme::Fonts(fonts));
        cx.set_global(settings::Ui { prefs, tour: None, startup_notice });

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1280.), px(820.)), cx)),
            titlebar: Some(TitlebarOptions { title: Some("kubernet-wizard".into()), ..Default::default() }),
            // Matches the Linux .desktop file so launchers show the app icon.
            app_id: Some("kubernet-wizard".to_owned()),
            ..Default::default()
        };
        let opened = gpui_kit::open_window(options, cx, |window, cx| {
            theme::apply(Some(window), cx);
            window.observe_window_appearance(|window, cx| theme::apply(Some(window), cx)).detach();
            cx.new(|cx| app::KwApp::new(window, cx))
        });
        if let Err(e) = opened {
            eprintln!("kubernet-wizard: cannot open window: {e}");
            cx.quit();
        }
    });
}
