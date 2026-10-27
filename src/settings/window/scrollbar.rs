//! A six-pixel, trackless thumb. GPUI-component fixes its always-visible thumb
//! at eight pixels; this small adapter keeps the requested geometry without a
//! dependency fork, timers, or another entity. Motion lives in `scrolling`.
use super::*;
use gpui::{
    DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollHandle, ScrollWheelEvent, point,
};

#[derive(Clone, Copy)]
struct Thumb {
    track: Bounds<Pixels>,
    bounds: Bounds<Pixels>,
    maximum: f32,
    travel: f32,
}

fn thumb(viewport: Bounds<Pixels>, handle: &ScrollHandle) -> Option<Thumb> {
    let maximum = f32::from(handle.max_offset().y);
    let height = f32::from(viewport.size.height) - 4.0;
    if maximum <= 0.0 || height <= 0.0 {
        return None;
    }
    let thumb_height = (height * height / (height + maximum)).max(24.0).min(height);
    let travel = height - thumb_height;
    let fraction = (-f32::from(handle.offset().y) / maximum).clamp(0.0, 1.0);
    let track = Bounds::new(
        point(viewport.right() - px(12.0), viewport.top() + px(2.0)),
        size(px(12.0), px(height)),
    );
    Some(Thumb {
        track,
        bounds: Bounds::new(
            point(
                viewport.right() - px(7.0),
                track.top() + px(travel * fraction),
            ),
            size(px(6.0), px(thumb_height)),
        ),
        maximum,
        travel,
    })
}

pub(super) fn overlay(cx: &Context<SettingsView>, handle: &ScrollHandle) -> impl IntoElement {
    let view = cx.entity().downgrade();
    let layout_handle = handle.clone();
    gpui::canvas(
        move |bounds, _, _| (bounds, thumb(bounds, &layout_handle)),
        move |_, (viewport, thumb), window, cx| {
            // Capture before the scrollable div's default bubble handler, exactly
            // like launcher/results_view: wheel input must not jump twice.
            let wheel_view = view.clone();
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && viewport.contains(&event.position) {
                    let _ =
                        wheel_view.update(cx, |view, cx| view.scrolling.wheel(event, window, cx));
                }
            });
            // Geometry is read after content layout. No overflow means neither a
            // painted scrollbar nor an invisible draggable hit area in this frame.
            let Some(thumb) = thumb else {
                return;
            };
            let palette = crate::theme::settings_palette(cx);
            let active = view
                .read_with(cx, |view, _| {
                    view.scrolling.hovered || view.scrolling.drag_anchor.is_some()
                })
                .unwrap_or(false);
            window.paint_quad(gpui::PaintQuad {
                corner_radii: px(3.0).into(),
                ..gpui::fill(
                    thumb.bounds,
                    if active {
                        palette.muted
                    } else {
                        palette.border
                    },
                )
            });

            let down_view = view.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture
                    || event.button != MouseButton::Left
                    || !thumb.track.contains(&event.position)
                {
                    return;
                }
                let _ = down_view.update(cx, |view, cx| {
                    view.scrolling.cancel();
                    view.scrolling.drag_anchor = Some(if thumb.bounds.contains(&event.position) {
                        f32::from(event.position.y - thumb.bounds.top())
                    } else {
                        f32::from(thumb.bounds.size.height) / 2.0
                    });
                    drag(view, event.position, thumb);
                    cx.notify();
                });
                window.prevent_default();
                cx.stop_propagation();
            });
            let move_view = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                let _ = move_view.update(cx, |view, cx| {
                    let hovered = thumb.track.contains(&event.position);
                    let mut changed = hovered != view.scrolling.hovered;
                    view.scrolling.hovered = hovered;
                    if view.scrolling.drag_anchor.is_some() {
                        if event.dragging() {
                            drag(view, event.position, thumb);
                        } else {
                            view.scrolling.drag_anchor = None;
                        }
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                });
            });
            let up_view = view.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                    return;
                }
                let _ = up_view.update(cx, |view, cx| {
                    if view.scrolling.drag_anchor.take().is_some() {
                        cx.notify();
                    }
                });
            });
        },
    )
    .absolute()
    .size_full()
}

fn drag(view: &mut SettingsView, position: Point<Pixels>, thumb: Thumb) {
    let Some(anchor) = view.scrolling.drag_anchor else {
        return;
    };
    let fraction = if thumb.travel > 0.0 {
        ((f32::from(position.y - thumb.track.top()) - anchor) / thumb.travel).clamp(0.0, 1.0)
    } else {
        0.0
    };
    view.scrolling.set_offset(-fraction * thumb.maximum);
}
