//! Asset source: app logo + the extra Lucide icons used by the UI, layered
//! over gpui-kit's default icon bundle. Only listed icons are embedded.

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

gpui_kit::assets::icon_assets!(ExtraIcons, [
    Boxes, Building2, Circle, ClipboardCheck, Cloud, ExternalLink, Gauge, GitBranch, House, Laptop, Lock, Network, Puzzle, Rocket,
    Route, Server, Sparkles, WandSparkles, Waypoints,
]);

/// Path of the app logo for `img(...)`.
pub const LOGO: &str = "kw/logo.png";
const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");

pub struct KwAssets;

impl AssetSource for KwAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(LOGO_PNG)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        if LOGO.starts_with(path) {
            paths.push(LOGO.into());
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
