# Bundled theme palettes

Hayai adapts upstream palette colors to launcher and settings surfaces, without
bundling editor plugins, logos, fonts or screenshots. Original copyright and
license texts are preserved in each source directory.

| Family | Variants | Source | License |
| --- | --- | --- | --- |
| One Dark | One Light / One Dark | [One Dark](https://github.com/atom/one-dark-syntax), [One Light](https://github.com/atom/one-light-syntax) | MIT |
| Catppuccin | Latte / Mocha | [Catppuccin](https://github.com/catppuccin/catppuccin) | MIT |
| Tokyo Night | Day / Night | [Tokyo Night](https://github.com/folke/tokyonight.nvim) | Apache-2.0 |
| Gruvbox | Light / Dark | [Gruvbox.nvim](https://github.com/ellisonleao/gruvbox.nvim) | MIT |

`src/theme.rs` maps base colors to Hayai roles. Selection, hover and border
surfaces are adapted for launcher controls. Preserve the modification notice
and the upstream licenses when redistributing these palettes.

`THIRD_PARTY_NOTICES` combines these licenses with the Phosphor and Scry artwork
notices. It is embedded for About and staged beside the executable for installer
and portable distribution. Update that file when sources change.
