use std::borrow::Cow;

pub struct LauncherAssets;

pub mod icons {
    // The same catalog defines names, embedded data, and AssetSource listing.
    macro_rules! svg_icons {
        ($($name:ident => $path:literal),* $(,)?) => {
            $(pub const $name: &str = $path;)*
            pub(super) const SVG_ASSETS: &[(&str, &[u8])] = &[
                $(($name, include_bytes!(concat!("../assets/", $path)))),*
            ];
        };
    }

    svg_icons! {
        ENTER => "icons/enter.svg",
        CALCULATOR => "icons/calculator.svg",
        ARROW_RIGHT => "icons/arrow-right.svg",
        UPDATE => "icons/update.svg",
        SPINNER => "icons/spinner.svg",
        SCRY => "icons/scry.svg",
        SEARCH_EMPTY => "icons/search-empty.svg",
        CALCULATOR_INVALID => "icons/calculator-invalid.svg",
        APPLICATION => "icons/application.svg",
    }

    pub const BRAND_PNG: &[u8] = include_bytes!("../assets/hayai.png");
    pub const UPDATE_PNG: &[u8] = include_bytes!("../assets/icons/update.png");
    pub const SCRY_PNG: &[u8] = include_bytes!("../assets/icons/scry.png");
}

impl gpui::AssetSource for LauncherAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(icons::SVG_ASSETS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        Ok(icons::SVG_ASSETS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect())
    }
}
