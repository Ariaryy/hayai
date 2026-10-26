use super::*;

/// Registered system shortcuts arrive through WM_HOTKEY instead of GPUI's
/// keystroke stream. Record that combination without opening the launcher.
/// Tray clicks still work while recording.
pub fn capture_registered_shortcut(cx: &mut App) -> bool {
    let Some(view) = cx.global::<SettingsGlobal>().view.clone() else {
        return false;
    };
    view.update(cx, |view, cx| {
        if view.recording.is_none() {
            return false;
        }
        let key = crate::settings::preferences(cx)
            .binding(Shortcut::ToggleLauncher)
            .to_owned();
        view.record_binding(&key, cx);
        true
    })
    .unwrap_or(false)
}

impl SettingsView {
    pub(super) fn subscriptions(window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let owner = window.window_handle();
        // Zed's keystroke_input intercepts before keymap dispatch. Scope this
        // app-wide hook to its own window so the launcher keeps its input.
        let listener = cx.listener(move |this, event: &gpui::KeystrokeEvent, window, cx| {
            if window.window_handle() != owner || this.recording.is_none() {
                return;
            }
            cx.stop_propagation();
            if event.keystroke.key == "escape" {
                this.cancel_recording();
                cx.notify();
            } else if !event.keystroke.key.is_empty()
                && !matches!(
                    event.keystroke.key.as_str(),
                    "control" | "shift" | "alt" | "platform" | "function"
                )
            {
                this.record_binding(&event.keystroke.unparse(), cx);
            }
        });
        let intercept = cx.intercept_keystrokes(listener);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.cancel_recording();
                this.scrolling.cancel();
                cx.notify();
            }
        });
        vec![intercept, activation]
    }

    fn record_binding(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(action) = self.recording else {
            return;
        };
        let mut next = preferences(cx).clone();
        match next.set_binding(action, key) {
            Ok(()) => {
                self.cancel_recording();
                save(next, cx);
            }
            Err(error) => self.validation = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(super) fn start_recording(
        &mut self,
        action: Shortcut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cx.global::<SettingsGlobal>().saving {
            return;
        }
        self.cancel_recording();
        self.recording = Some(action);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn reset_binding(&mut self, action: Shortcut, cx: &mut Context<Self>) {
        if cx.global::<SettingsGlobal>().saving {
            return;
        }
        self.cancel_recording();
        let mut next = preferences(cx).clone();
        match next.set_binding(action, action.default_key()) {
            Ok(()) => save(next, cx),
            Err(error) => self.validation = Some(error.to_string()),
        }
        cx.notify();
    }

    pub(super) fn cancel_recording(&mut self) {
        self.recording = None;
        self.validation = None;
    }
}
