//! Asset and palette notices only; this is not an inventory of Cargo packages.
//! Texts remain beside their sources and are also shipped in THIRD_PARTY_NOTICES.

pub struct Notice {
    pub name: &'static str,
    pub version: Option<&'static str>,
    pub license_id: &'static str,
    pub category: &'static str,
    pub source: &'static str,
    pub attribution: &'static str,
    pub license_text: &'static str,
}

pub const NOTICES: &[Notice] = &[
    Notice {
        name: "Catppuccin",
        version: None,
        license_id: "MIT",
        category: crate::ui_text::settings::LICENSE_PALETTE,
        source: "https://github.com/catppuccin/catppuccin",
        attribution: "Latte and Mocha palette colors adapted to Hayai's launcher and settings surfaces.",
        license_text: include_str!("../assets/themes/catppuccin/LICENSE"),
    },
    Notice {
        name: "Gruvbox.nvim",
        version: None,
        license_id: "MIT",
        category: crate::ui_text::settings::LICENSE_PALETTE,
        source: "https://github.com/ellisonleao/gruvbox.nvim",
        attribution: "Light and dark palette colors adapted to Hayai's launcher and settings surfaces.",
        license_text: include_str!("../assets/themes/gruvbox/LICENSE"),
    },
    Notice {
        name: "One Dark",
        version: None,
        license_id: "MIT",
        category: crate::ui_text::settings::LICENSE_PALETTE,
        source: "https://github.com/atom/one-dark-syntax",
        attribution: "Base palette adapted to Hayai's launcher and settings surfaces.",
        license_text: include_str!("../assets/themes/one-dark/LICENSE"),
    },
    Notice {
        name: "One Light",
        version: None,
        license_id: "MIT",
        category: crate::ui_text::settings::LICENSE_PALETTE,
        source: "https://github.com/atom/one-light-syntax",
        attribution: "Base palette adapted to Hayai's launcher and settings surfaces.",
        license_text: include_str!("../assets/themes/one-light/LICENSE"),
    },
    Notice {
        name: "Phosphor Icons",
        version: None,
        license_id: "MIT",
        category: crate::ui_text::settings::LICENSE_ARTWORK,
        source: "https://github.com/phosphor-icons/core",
        attribution: "Plain navigation and appearance icons, using the original SVG paths.",
        license_text: include_str!("../assets/icons/thirdparty/phosphor/LICENSE"),
    },
    Notice {
        name: "Scry Search",
        version: Some("0.1.1"),
        license_id: "MIT",
        category: crate::ui_text::settings::LICENSE_ARTWORK,
        source: "https://github.com/Ariaryy/scry-search/tree/v0.1.1",
        attribution: "Scry application icon and its raster rendition.",
        license_text: include_str!("../assets/icons/thirdparty/scry/LICENSE"),
    },
    Notice {
        name: "Tokyo Night",
        version: None,
        license_id: "Apache-2.0",
        category: crate::ui_text::settings::LICENSE_PALETTE,
        source: "https://github.com/folke/tokyonight.nvim",
        attribution: "Modified for Hayai: Day and Night colors mapped to launcher and settings roles; selection, border and hover surfaces adapted. No upstream code is bundled.",
        license_text: include_str!("../assets/themes/tokyonight/LICENSE"),
    },
];

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_notices_match_the_distributed_license_bundle() {
        // Windows checkout/download line endings do not change license text.
        let bundle = include_str!("../THIRD_PARTY_NOTICES").replace("\r\n", "\n");
        for notice in super::NOTICES {
            assert!(
                bundle.contains(&notice.license_text.replace("\r\n", "\n")),
                "{} license missing from distribution notices",
                notice.name
            );
            assert!(
                bundle.contains(notice.source),
                "{} source missing from distribution notices",
                notice.name
            );
        }
    }
}
