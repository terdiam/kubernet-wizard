//! App update flow: check → (download) → verify → install → relaunch.
//! Network and disk work run on background executors; progress is shared
//! lock-free through `kw_update::Progress`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::progress::Progress as ProgressBar;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::text::TextView;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::base::StyledExt as _;
use kw_update::{Channel, Progress, Update};

use crate::app::KwApp;
use crate::i18n::{self, K};
use crate::settings::{self, Ui};

#[derive(Clone)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(Arc<Update>),
    Downloading(Arc<Update>, Arc<Progress>),
    Ready(Arc<Update>, PathBuf),
    Error(SharedString),
}

#[inline]
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Loads the verified cached catalog into the distro registry (startup).
pub fn install_cached_catalog() {
    if let Some(c) = kw_update::load_cached_catalog(&settings::catalog_dir()) {
        apply_catalog(c);
    }
}

fn apply_catalog(c: kw_update::Catalog) {
    let entries = c.distros.into_iter().filter_map(|(id, versions)| {
        serde_json::from_value::<kw_core::DistroId>(serde_json::Value::String(id)).ok().map(|id| (id, versions))
    });
    kw_distro::set_catalog(entries);
}

impl KwApp {
    /// Startup hook: automatic check (throttled) + catalog refresh.
    pub(crate) fn auto_update_check(&mut self, cx: &mut Context<Self>) {
        let prefs = &cx.global::<Ui>().prefs.update;
        if prefs.auto_check && now().saturating_sub(prefs.last_check) >= kw_update::CHECK_INTERVAL_SECS {
            self.check_updates(false, cx);
        }
        // Catalog applies on next start (registry is set once per process).
        cx.background_spawn(async move {
            let _ = kw_update::fetch_catalog(kw_update::RELEASES, &settings::catalog_dir());
        })
        .detach();
    }

    pub(crate) fn check_updates(&mut self, manual: bool, cx: &mut Context<Self>) {
        if matches!(self.update, UpdateState::Checking | UpdateState::Downloading(..)) {
            return;
        }
        self.update = UpdateState::Checking;
        // `KW_UPDATE_URL` overrides the feed for local testing.
        let manifest_url = std::env::var("KW_UPDATE_URL").unwrap_or_else(|_| cx.global::<Ui>().prefs.update.channel.manifest_url().to_owned());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { kw_update::check(&manifest_url, &kw_update::current_version()) })
                .await;
            let _ = this.update(cx, |app, cx| {
                {
                    let ui = cx.global_mut::<Ui>();
                    ui.prefs.update.last_check = now();
                    settings::save(&ui.prefs);
                }
                let lang = cx.global::<Ui>().prefs.lang;
                app.update = match res {
                    Ok(Some(u)) => {
                        app.pending_notice = Some(format!("{} — v{}", i18n::t(lang, K::UpdateAvailable), u.version).into());
                        UpdateState::Available(Arc::new(u))
                    }
                    Ok(None) => UpdateState::UpToDate,
                    // Silent on automatic checks (offline etc.), visible on manual.
                    Err(e) if manual => UpdateState::Error(e.to_string().into()),
                    Err(_) => UpdateState::Idle,
                };
                if cx.global::<Ui>().prefs.update.auto_download && matches!(app.update, UpdateState::Available(_)) {
                    app.download_update(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn download_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(update) = &self.update else { return };
        if package_managed() {
            return;
        }
        let update = Arc::clone(update);
        let progress = Arc::new(Progress::default());
        self.update = UpdateState::Downloading(Arc::clone(&update), Arc::clone(&progress));
        cx.notify();

        let (job_update, job_progress) = (Arc::clone(&update), Arc::clone(&progress));
        let job = cx.background_spawn(async move {
            kw_update::download(&job_update.asset, &settings::updates_dir(), &job_progress)
        });
        // Repaint progress while downloading.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let still = this
                    .update(cx, |app, cx| {
                        cx.notify();
                        matches!(app.update, UpdateState::Downloading(..))
                    })
                    .unwrap_or(false);
                if !still {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            let res = job.await;
            let _ = this.update(cx, |app, cx| {
                app.update = match res {
                    Ok(file) => UpdateState::Ready(update, file),
                    Err(e) => UpdateState::Error(e.to_string().into()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn cancel_download(&mut self, cx: &mut Context<Self>) {
        if let UpdateState::Downloading(u, p) = &self.update {
            p.cancel.store(true, Ordering::Relaxed);
            self.update = UpdateState::Available(Arc::clone(u));
            cx.notify();
        }
    }

    /// Installs the verified artifact and relaunches. Refused while deploying.
    pub(crate) fn install_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lang = cx.global::<Ui>().prefs.lang;
        if self.wizard.read(cx).is_busy() {
            window.push_notification(i18n::t(lang, K::UpdateBusy), cx);
            return;
        }
        let UpdateState::Ready(update, file) = &self.update else { return };
        let install = kw_update::detect_install();
        let version = update.version.to_string();
        match kw_update::apply(file, update.asset.kind, &install, &version, &settings::state_dir())
            .and_then(|target| kw_update::relaunch(&target))
        {
            Ok(()) => cx.quit(),
            Err(e) => {
                self.update = UpdateState::Error(e.to_string().into());
                cx.notify();
            }
        }
    }

    /// Header button reflecting update state (None when nothing to show).
    pub(crate) fn update_button(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let lang = cx.global::<Ui>().prefs.lang;
        let (label, icon): (SharedString, IconName) = match &self.update {
            UpdateState::Available(u) => (format!("v{}", u.version).into(), IconName::ArrowDown),
            UpdateState::Downloading(_, p) => (pct(p).into(), IconName::Loader),
            UpdateState::Ready(..) => (i18n::t(lang, K::UpdateRestart).into(), IconName::RefreshCw),
            _ => return None,
        };
        let p = crate::theme::pal(cx);
        Some(
            h_flex()
                .id("update-indicator")
                .gap_1()
                .items_center()
                .px(px(10.))
                .py(px(4.))
                .rounded_full()
                .bg(p.gds)
                .text_color(p.gdt)
                .text_xs()
                .font_semibold()
                .cursor_pointer()
                .child(Icon::new(icon).xsmall())
                .child(label)
                .tooltip(move |window, cx| Tooltip::new(i18n::t(lang, K::UpdateAvailable)).build(window, cx))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_update_dialog(window, cx))),
        )
    }

    pub(crate) fn open_update_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(app) = this.upgrade() else { return dialog };
            let lang = cx.global::<Ui>().prefs.lang;
            let state = app.read(cx).update.clone();
            let muted = cx.theme().muted_foreground;
            let (version, notes) = match &state {
                UpdateState::Available(u) | UpdateState::Downloading(u, _) | UpdateState::Ready(u, _) => {
                    (format!("v{}", u.version), u.notes.clone())
                }
                _ => (String::new(), String::new()),
            };
            let current = kw_update::current_version().to_string();
            let body = v_flex()
                .gap_3()
                .child(div().text_sm().text_color(muted).child(format!("{}: v{current} → {version}", i18n::t(lang, K::CurrentVersion))))
                .when(!notes.is_empty(), |d| {
                    d.child(div().id("update-notes").max_h(px(260.)).overflow_y_scroll().child(TextView::markdown("update-notes-md", notes)))
                })
                .when_some(
                    match &state {
                        UpdateState::Downloading(_, p) => Some(p),
                        _ => None,
                    },
                    |d, p| d.child(ProgressBar::new("update-progress").value(pct_value(p))).child(div().text_xs().child(pct(p))),
                )
                .when(matches!(state, UpdateState::Ready(..)), |d| {
                    d.child(div().text_xs().text_color(cx.theme().success).child(i18n::t(lang, K::UpdateVerified)))
                })
                .when_some(
                    match &state {
                        UpdateState::Error(e) => Some(e.clone()),
                        _ => None,
                    },
                    |d, e| d.child(div().text_sm().text_color(cx.theme().danger).child(e)),
                );
            let pkg = package_managed();
            let body = body.when(pkg && matches!(state, UpdateState::Available(_)), |d| {
                d.child(div().text_sm().child(i18n::t(lang, K::UpdateViaPackage)))
            });
            let action = {
                let this = this.clone();
                match &state {
                    UpdateState::Available(u) if pkg => {
                        let url = format!("{}/tag/v{}", RELEASES_PAGE, u.version);
                        Some(Button::new("upd-release").primary().icon(IconName::ExternalLink).label(i18n::t(lang, K::UpdateOpenRelease)).on_click(
                            move |_, _, cx| cx.open_url(&url),
                        ))
                    }
                    UpdateState::Available(_) => Some(Button::new("upd-download").primary().icon(IconName::ArrowDown).label(i18n::t(lang, K::UpdateDownload)).on_click(
                        move |_, _, cx| {
                            let _ = this.update(cx, |a, cx| a.download_update(cx));
                        },
                    )),
                    UpdateState::Downloading(..) => Some(Button::new("upd-cancel").outline().label(i18n::t(lang, K::Cancel)).on_click(move |_, _, cx| {
                        let _ = this.update(cx, |a, cx| a.cancel_download(cx));
                    })),
                    UpdateState::Ready(..) => Some(Button::new("upd-install").primary().icon(IconName::RefreshCw).label(i18n::t(lang, K::UpdateRestart)).on_click(
                        move |_, window, cx| {
                            let _ = this.update(cx, |a, cx| a.install_update(window, cx));
                        },
                    )),
                    _ => None,
                }
            };
            dialog
                .title(format!("{} {version}", i18n::t(lang, K::UpdateAvailable)))
                .w(px(520.))
                .child(body)
                .footer(h_flex().justify_end().gap_2().children(action))
        });
    }

    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let Some(app) = this.upgrade() else { return dialog };
            let lang = cx.global::<Ui>().prefs.lang;
            let prefs = cx.global::<Ui>().prefs.update.clone();
            let state = app.read(cx).update.clone();
            let muted = cx.theme().muted_foreground;
            let status: SharedString = match &state {
                UpdateState::Checking => i18n::t(lang, K::UpdateChecking).into(),
                UpdateState::UpToDate => i18n::t(lang, K::UpdateUpToDate).into(),
                UpdateState::Available(u) | UpdateState::Downloading(u, _) | UpdateState::Ready(u, _) => {
                    format!("{} — v{}", i18n::t(lang, K::UpdateAvailable), u.version).into()
                }
                UpdateState::Error(e) => e.clone(),
                UpdateState::Idle => SharedString::default(),
            };
            let set = |f: fn(&mut settings::UpdatePrefs, bool)| {
                move |v: &bool, _: &mut Window, cx: &mut App| {
                    let ui = cx.global_mut::<Ui>();
                    f(&mut ui.prefs.update, *v);
                    settings::save(&ui.prefs);
                    cx.refresh_windows();
                }
            };
            let channel_btn = |c: Channel| {
                Button::new(("channel", c as usize))
                    .small()
                    .label(c.label())
                    .selected(prefs.channel == c)
                    .when(prefs.channel == c, |b| b.primary())
                    .when(prefs.channel != c, |b| b.outline())
                    .on_click(move |_, _, cx| {
                        let ui = cx.global_mut::<Ui>();
                        ui.prefs.update.channel = c;
                        settings::save(&ui.prefs);
                        cx.refresh_windows();
                    })
            };
            let check_now = {
                let this = this.clone();
                Button::new("upd-check-now")
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(i18n::t(lang, K::UpdateCheckNow))
                    .disabled(matches!(state, UpdateState::Checking | UpdateState::Downloading(..)))
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |a, cx| a.check_updates(true, cx));
                    })
            };
            dialog.title(i18n::t(lang, K::Settings)).w(px(480.)).child(
                v_flex()
                    .gap_3()
                    .child(div().text_sm().text_color(muted).child(format!("{}: v{}", i18n::t(lang, K::CurrentVersion), kw_update::current_version())))
                    .child(Switch::new("upd-auto-check").label(i18n::t(lang, K::UpdateAutoCheck)).checked(prefs.auto_check).on_change(set(|p, v| p.auto_check = v)))
                    .child(
                        Switch::new("upd-auto-dl")
                            .label(i18n::t(lang, K::UpdateAutoDownload))
                            .checked(prefs.auto_download)
                            .on_change(set(|p, v| p.auto_download = v)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().text_sm().font_medium().child(i18n::t(lang, K::UpdateChannel)))
                            .child(channel_btn(Channel::Stable))
                            .child(channel_btn(Channel::Beta)),
                    )
                    .child(h_flex().gap_2().items_center().child(check_now).child(div().text_sm().text_color(muted).child(status))),
            )
        });
    }
}

#[inline]
fn pct_value(p: &Progress) -> f32 {
    let total = p.total.load(Ordering::Relaxed);
    if total == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let v = p.done.load(Ordering::Relaxed) as f32 / total as f32 * 100.0;
    v.min(100.0)
}

fn pct(p: &Progress) -> String {
    let done_mb = p.done.load(Ordering::Relaxed) / 1_048_576;
    let total = p.total.load(Ordering::Relaxed);
    if total == 0 {
        format!("{done_mb} MB")
    } else {
        format!("{:.0}% · {done_mb}/{} MB", pct_value(p), total / 1_048_576)
    }
}

const RELEASES_PAGE: &str = "https://github.com/terdiam/kubernet-wizard/releases";

/// deb/rpm installs are updated by the package manager, not in place.
#[inline]
fn package_managed() -> bool {
    matches!(kw_update::detect_install(), kw_update::Install::PackageManaged)
}
