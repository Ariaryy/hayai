use super::*;

pub(in crate::launcher) fn search_bar(
    root: &LauncherRoot,
    cx: &Context<LauncherRoot>,
) -> impl IntoElement {
    div()
        .flex_none()
        .h(px(52.0))
        .w_full()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            Input::new(&root.search_input)
                .flex_1()
                .min_w_0()
                .appearance(false)
                .with_size(Size::Large)
                .h(px(52.0))
                .text_size(px(16.0))
                // `Input`'s own internal render sets a fixed
                // `line_height(1.25rem)` unconditionally; that's
                // too tight for some glyphs' descenders (e.g. "g")
                // at this font size and clips them. Overriding it
                // here works because `refine_style` only replaces
                // fields we actually set, and unlike `text_size`,
                // we weren't setting this one before.
                .line_height(relative(1.5)),
        )
        .when(!root.folder_scope.is_empty(), |header| {
            let mut breadcrumbs = h_flex()
                .flex_none()
                .h(px(32.0))
                .max_w(px(240.0))
                .px_3()
                .gap_1()
                .overflow_hidden()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    div()
                        .id("scope-all-files")
                        .flex_none()
                        .cursor_pointer()
                        .hover(|el| el.text_color(cx.theme().foreground))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.folder_scope.clear();
                                this.refresh_folder_scope(window, cx);
                                this.search_input
                                    .update(cx, |input, cx| input.focus(window, cx));
                            }),
                        )
                        .child(text::search::ALL_FILES),
                );
            for (index, folder) in root.folder_scope.iter().enumerate() {
                let name = folder
                    .file_name()
                    .unwrap_or(folder.as_os_str())
                    .to_string_lossy()
                    .into_owned();
                let current = index + 1 == root.folder_scope.len();
                breadcrumbs = breadcrumbs.child("/").child(
                    div()
                        .id(("scope-folder", index))
                        .min_w_0()
                        .truncate()
                        .cursor_pointer()
                        .when(current, |el| el.text_color(cx.theme().foreground))
                        .hover(|el| el.text_color(cx.theme().foreground))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.folder_scope.truncate(index + 1);
                                this.refresh_folder_scope(window, cx);
                                this.search_input
                                    .update(cx, |input, cx| input.focus(window, cx));
                            }),
                        )
                        .child(name),
                );
            }
            header.child(breadcrumbs)
        })
}
