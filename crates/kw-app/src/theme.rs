//! Brand palette from the Claude Design handoff (`design/Kubernet Wizard.dc.html`).
//!
//! The gpui-kit theme gets the base colors; the extra soft/strong pairs used
//! for badges, selected chips and status text live in [`Palette`] (a global).
// Hex literals mirror the design file 1:1.
#![allow(clippy::unreadable_literal)]

use gpui_kit::component::{Theme, ThemeMode};

use crate::settings::{ThemeChoice, Ui};
use gpui_kit::*;

/// Design tokens. `*s` = soft background, `*t` = text on that background.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Hsla,
    pub sf: Hsla,
    pub s2: Hsla,
    pub bd: Hsla,
    pub tx: Hsla,
    pub mu: Hsla,
    pub pr: Hsla,
    pub prt: Hsla,
    pub prs: Hsla,
    pub okt: Hsla,
    pub oks: Hsla,
    pub wnt: Hsla,
    pub wns: Hsla,
    pub ert: Hsla,
    pub ers: Hsla,
    pub er: Hsla,
    pub gds: Hsla,
    pub gdt: Hsla,
}

impl Global for Palette {}

/// Whether the embedded fonts registered.
pub struct Fonts(pub bool);

impl Global for Fonts {}

#[inline]
fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                bg: c(0x0E1220),
                sf: c(0x151A2C),
                s2: c(0x1D2338),
                bd: c(0x2A3150),
                tx: c(0xE8EBF5),
                mu: c(0xA0A9C4),
                pr: c(0x4C6AE8),
                prt: c(0x9DB0FF),
                prs: c(0x1E2750),
                okt: c(0x4FD9C8),
                oks: c(0x0F2E2E),
                wnt: c(0xF3B84B),
                wns: c(0x33290F),
                ert: c(0xFF8A8A),
                ers: c(0x3A1B22),
                er: c(0xE5484D),
                gds: c(0x3A3210),
                gdt: c(0xFFD43B),
            }
        } else {
            Self {
                bg: c(0xF5F6FB),
                sf: c(0xFFFFFF),
                s2: c(0xEDF0F7),
                bd: c(0xD9DEEB),
                tx: c(0x1A2036),
                mu: c(0x586078),
                pr: c(0x3B5BDB),
                prt: c(0x2F49B5),
                prs: c(0xE8ECFC),
                okt: c(0x0A7A6F),
                oks: c(0xE1F7F4),
                wnt: c(0x9A5B00),
                wns: c(0xFFF3D1),
                ert: c(0xB42323),
                ers: c(0xFDECEC),
                er: c(0xD93636),
                gds: c(0xFFF3C2),
                gdt: c(0x6B5200),
            }
        }
    }
}

/// UI fonts from the design (SIL OFL 1.1, see `assets/fonts/OFL-*.txt`).
const SANS: &str = "Hanken Grotesk";
const MONO: &str = "JetBrains Mono";
const FONTS: [&[u8]; 6] = [
    include_bytes!("../assets/fonts/HankenGrotesk-Regular.ttf"),
    include_bytes!("../assets/fonts/HankenGrotesk-Medium.ttf"),
    include_bytes!("../assets/fonts/HankenGrotesk-SemiBold.ttf"),
    include_bytes!("../assets/fonts/HankenGrotesk-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf"),
];

/// Registers the embedded fonts once at startup. On failure the kit keeps
/// the system fonts.
pub fn load_fonts(cx: &App) -> bool {
    match cx.text_system().add_fonts(FONTS.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect()) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("kubernet-wizard: cannot load fonts: {e}");
            false
        }
    }
}

/// Current palette (installed by [`apply`]).
#[inline]
pub fn pal(cx: &App) -> Palette {
    *cx.global::<Palette>()
}

#[inline]
fn shade(c: Hsla, dl: f32) -> Hsla {
    Hsla { l: (c.l + dl).clamp(0., 1.), ..c }
}

/// Applies the chosen scheme (or the OS appearance), then paints the brand
/// palette over the kit theme.
pub fn apply(window: Option<&mut Window>, cx: &mut App) {
    let fonts_ok = cx.try_global::<Fonts>().is_some_and(|f| f.0);
    let choice = cx.try_global::<Ui>().map_or(ThemeChoice::System, |ui| ui.prefs.theme);
    match choice {
        ThemeChoice::System => Theme::sync_system_appearance(window, cx),
        ThemeChoice::Light => Theme::change(ThemeMode::Light, window, cx),
        ThemeChoice::Dark => Theme::change(ThemeMode::Dark, window, cx),
    }
    let dark = Theme::global(cx).is_dark();
    let p = Palette::new(dark);
    let white = c(0xFFFFFF);
    let hover = if dark { 0.06 } else { -0.06 };
    Theme::update(cx, |t| {
        let k = &mut t.colors;
        k.background = p.bg;
        k.foreground = p.tx;
        k.border = p.bd;
        k.input = p.bd;
        k.ring = p.pr;
        k.caret = p.pr;
        k.selection = p.prs;
        k.muted = p.s2;
        k.muted_foreground = p.mu;
        k.accent = p.prs;
        k.accent_foreground = p.prt;
        k.primary = p.pr;
        k.primary_hover = shade(p.pr, hover);
        k.primary_active = shade(p.pr, 2. * hover);
        k.primary_foreground = white;
        k.button_primary = p.pr;
        k.button_primary_hover = shade(p.pr, hover);
        k.button_primary_active = shade(p.pr, 2. * hover);
        k.button_primary_foreground = white;
        k.secondary = p.s2;
        k.secondary_hover = shade(p.s2, hover / 2.);
        k.secondary_active = shade(p.s2, hover);
        k.secondary_foreground = p.tx;
        k.danger = p.er;
        k.danger_foreground = white;
        k.success = p.okt;
        k.warning = p.wnt;
        k.link = p.prt;
        k.link_hover = p.pr;
        k.popover = p.sf;
        k.popover_foreground = p.tx;
        k.title_bar = p.sf;
        k.title_bar_border = p.bd;
        k.sidebar = p.sf;
        k.sidebar_border = p.bd;
        k.list = p.sf;
        k.list_hover = p.s2;
        k.list_active = p.prs;
        k.list_active_border = p.pr;
        k.tab_bar = p.s2;
        k.tab_active = p.sf;
        if fonts_ok {
            t.font_family = SANS.into();
            t.mono_font_family = MONO.into();
        }
        t.radius = px(8.);
        t.radius_lg = px(12.);
    });
    cx.set_global(p);
}
