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
        CHEVRON_DOWN => "icons/chevron-down.svg",
        CARET_RIGHT => "icons/thirdparty/phosphor/caret-right.svg",
        CARET_DOWN => "icons/thirdparty/phosphor/caret-down.svg",
        EXTERNAL_LINK => "icons/thirdparty/phosphor/arrow-square-out.svg",
        SUN => "icons/thirdparty/phosphor/sun.svg",
        MOON => "icons/thirdparty/phosphor/moon.svg",
        WARNING => "icons/thirdparty/phosphor/warning.svg",
        // TitleBar resolves these exact gpui-component IconName paths.
        WINDOW_CLOSE => "icons/window-close.svg",
        WINDOW_MINIMIZE => "icons/window-minimize.svg",
        WINDOW_MAXIMIZE => "icons/window-maximize.svg",
        WINDOW_RESTORE => "icons/window-restore.svg",
        SETTINGS_GENERAL => "icons/thirdparty/phosphor/settings-general.svg",
        SETTINGS_APPEARANCE => "icons/thirdparty/phosphor/settings-appearance.svg",
        SETTINGS_KEYBOARD => "icons/thirdparty/phosphor/settings-keyboard.svg",
        SETTINGS_FILES => "icons/thirdparty/phosphor/settings-files.svg",
        SETTINGS_ABOUT => "icons/thirdparty/phosphor/settings-about.svg",
        ENTER => "icons/enter.svg",
        CALCULATOR => "icons/calculator.svg",
        ARROW_RIGHT => "icons/arrow-right.svg",
        UPDATE => "icons/update.svg",
        SPINNER => "icons/spinner.svg",
        SCRY => "icons/thirdparty/scry/scry.svg",
        SEARCH_EMPTY => "icons/search-empty.svg",
        CALCULATOR_INVALID => "icons/calculator-invalid.svg",
        APPLICATION => "icons/application.svg",
    }

    pub const BRAND_PNG: &[u8] = include_bytes!("../assets/hayai.png");
    pub const UPDATE_PNG: &[u8] = include_bytes!("../assets/icons/update.png");
    pub const SCRY_PNG: &[u8] = include_bytes!("../assets/icons/thirdparty/scry/scry.png");
    pub const SETTINGS_PNG: &[u8] = include_bytes!("../assets/icons/settings.png");
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
