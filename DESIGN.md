# Hayai design

Visual and interaction rules for the settings window and the colors the
launcher shares with it. Launcher layout and list invariants live in
[GPUI and Windows invariants](docs/internal/gpui-windows.md); this file does
not restate them. Launcher spacing is settled: leave it as it is.

## Scale (settings window)

Constants live in `src/settings/window/components.rs`. Take values from there
and extend the scale there; a literal in a page file is a miss.

- **Grid**: 4px. Page padding 32px, content `max_w` 800px, sidebar padding 8px on all four sides.
- **Groups**: build every page from `group(title)`, a heading plus its cards at a 12px gap. `page()` spaces groups 32px apart, so a heading always sits closer to its own cards than to the previous group.
- **Radii**: `RADIUS_SM` 4 (keycaps, inline glyph hovers), `RADIUS_MD` 6 (controls, chips), `RADIUS_LG` 8 (nav items, list rows), `RADIUS_XL` 12 (cards). A nested radius is the parent's minus its padding (cards at 12 with `p_1` hold 8).
- **Type**: caption 12/20 (descriptions, hints), body 13/20, section heading 15/24 semibold, About title 20/28. Keycaps use 11. Set size and line height together.
- **Rows**: `setting_row` and shortcut rows share `ROW_MIN_HEIGHT` 72 and 16px padding, so every card reads at one rhythm.
- **Controls**: buttons and dropdowns are `CONTROL_HEIGHT` 28. Text inputs keep gpui-component's small height (24); chips, the trigger field and the add field match it through `INPUT_HEIGHT` in `commands.rs`.
- **Alignment**: headings align to the card edge; the breadcrumb aligns to page padding; nav item text aligns to the brand header.

## Pixel alignment

Windows scales from 100% up (125%, 150%, custom), so a logical size is a whole number of device pixels only sometimes, and centering leaves half a pixel when the free space is odd. The renderer then rounds each edge on its own and the child lands a pixel off-center, usually toward the top left. `src/pixels.rs` removes this once, for every scale, so widgets never carry per-widget nudges.

- **Sync**: each window's `render` calls `pixels::sync(window, cx)` first; helpers read the scale from it.
- **Icons**: place a fixed-size icon with `pixels::fit(cx, container, border_dev, icon)`. It returns the size and offset; the icon changes by at most one device pixel so the free space is even.
- **Exact boxes**: `snap` rounds a size to whole device pixels, `lead` gives the offset that centers a child in a box, `hairline` is one device pixel.
- **Text**: a line box centers text by font metrics (ascent plus descent), not by the glyphs people see. `pixels::text_top` lifts the line by the metric difference for the glyph kind (`Glyphs::of(label)` picks capitals, lowercase or symbols such as `=`). Calibrated for Segoe UI.
- **Widgets**: `components::keycap_label`, `keycap_icon` and `ToggleSwitch`, the alias chips and the nav items already use these. gpui-component's `Switch` insets its thumb by 2px, which is off-center at 125%, so use ours.
- **Layout**: place centered fixed-size children with margins from these helpers, not `items_center`.
- **Tests**: `pixels.rs` asserts whole-pixel, balanced offsets at 100%-400% in 25% steps and fractional scales. Add a case when you add a helper.
- **Verify**: render the page, capture the window at the real scale, and measure the ink against its box; gaps should match within one device pixel. Compare before and after.

## Color

`launcher_palette` in `src/theme.rs` is the single source; the launcher, settings and previews read it.

- **Surfaces**: sidebar (recessed) < canvas < card (raised) < popover (menus, tooltips, the action panel). Each derives from the theme background, so every theme gets the same order. Hover and selection steps stay on the upstream palette.
- **Legibility**: text, muted, danger need 4.5:1 and accent, warning need 3:1 against every surface. `readable` nudges a role toward the foreground until it passes, and the test in `theme.rs` covers all ten palettes. A new theme or role joins that test.
- **Roles**: use `accent` for selection and active controls, `warning` for conflict glyphs, `danger` for error text. The titlebar close button reads the theme's own `danger` tokens, which are fixed to #e81123 (hover), #8a0a15 (pressed) and a white glyph; `SettingsPalette::danger` comes from the palette instead.
- **Blending**: `mix_oklab(other, f)` weights the receiver by `f`. Use `blend(from, to, amount)` in `theme.rs` so the amount reads as distance traveled.

## Conflicts

Conflicts are saved and flagged, never refused. A conflicted entry is inactive until the user changes it.

- **Marker**: an amber `WARNING` glyph at the leading edge of what it flags, with a tooltip stating the clash and its effect. Use `conflict_slot` where a row reserves the space (shortcuts, trigger) and `conflict_glyph` where the item is variable (alias chips). A reserved slot keeps its width when empty so rows stay aligned.
- **Tooltips**: `Tooltip::element` with `max_w` 260 so long text wraps.
- **Format errors** (bad characters, too long) stay inline under the row in `danger` and block closing the window. They differ from conflicts because the value cannot be saved.
- **Shortcuts**: the system-wide hotkey keeps working in a conflict; the in-launcher action it shares a key with is disabled.

## Commands

- **Trigger**: one to four keys typed after Space, shown as `Space` plus a single badge (`FE`, not `F` `E`). The launcher picker omits `Space`; the user already pressed it.
- **Aliases**: full-word chips, at most eight. At the limit the add field disappears and no error appears.
- **Picker**: on by default. With it hidden, text after Space that can no longer become a command is a normal search.
- **Overlap rule**: entries of different commands may not be prefixes of one another.

## Change checklist

- Take sizes, radii and colors from the scales above.
- Verify both color modes and at least one tinted theme (Catppuccin, Gruvbox).
- Compare before and after captures for spacing changes, per AGENTS.md.
- Add user-visible changes to `CHANGELOG.md`.
