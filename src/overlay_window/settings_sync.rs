use super::state::AppConnectionState;
use super::OverlayApp;
use crate::keyboard::VisibilityOptions;
use crate::settings::{ProtocolType, WindowPosition};
use eframe::egui::{self, Align2};
use std::time::{Duration, Instant};

impl OverlayApp {
    pub(super) fn apply_live_visual_settings(&mut self) {
        let old_timeout = self.settings.active.timeout;
        let old_visibility = (
            self.settings.active.show_on_non_base_layer,
            self.settings.active.show_on_key_held,
            self.settings.active.hold_threshold_ms,
        );
        let changed = self.settings.active.size != self.settings.draft.size
            || self.settings.active.font_size_multiplier
                != self.settings.draft.font_size_multiplier
            || self.settings.active.auto_fit_before_ellipsis
                != self.settings.draft.auto_fit_before_ellipsis
            || self.settings.active.margin != self.settings.draft.margin
            || self.settings.active.position != self.settings.draft.position
            || self.settings.active.timeout != self.settings.draft.timeout
            || self.settings.active.theme != self.settings.draft.theme
            || self.settings.active.show_on_non_base_layer
                != self.settings.draft.show_on_non_base_layer
            || self.settings.active.show_on_key_held != self.settings.draft.show_on_key_held
            || self.settings.active.hold_threshold_ms != self.settings.draft.hold_threshold_ms
            || self.settings.active.show_hold_annotation
                != self.settings.draft.show_hold_annotation;

        if !changed {
            return;
        }

        self.settings.active.size = self.settings.draft.size;
        self.settings.active.font_size_multiplier = self.settings.draft.font_size_multiplier;
        self.settings.active.auto_fit_before_ellipsis =
            self.settings.draft.auto_fit_before_ellipsis;
        self.settings.active.margin = self.settings.draft.margin;
        self.settings.active.position = self.settings.draft.position;
        self.settings.active.timeout = self.settings.draft.timeout;
        self.settings.active.theme = self.settings.draft.theme.clone();
        self.settings.active.show_on_non_base_layer =
            self.settings.draft.show_on_non_base_layer;
        self.settings.active.show_on_key_held = self.settings.draft.show_on_key_held;
        self.settings.active.hold_threshold_ms = self.settings.draft.hold_threshold_ms;
        self.settings.active.show_hold_annotation = self.settings.draft.show_hold_annotation;

        if let AppConnectionState::Connected { keyboard } = &self.session.connection {
            if old_timeout != self.settings.active.timeout {
                keyboard.set_timeout(self.settings.active.timeout);
            }
            let new_visibility = (
                self.settings.active.show_on_non_base_layer,
                self.settings.active.show_on_key_held,
                self.settings.active.hold_threshold_ms,
            );
            if old_visibility != new_visibility {
                keyboard.set_visibility_options(VisibilityOptions {
                    show_on_non_base_layer: new_visibility.0,
                    show_on_key_held: new_visibility.1,
                    hold_threshold_ms: new_visibility.2,
                });
            }
        }

        self.persist_settings();
    }

    pub(super) fn apply_live_layout_settings(&mut self) {
        if self.session.active_layout_name == self.session.draft_layout_name {
            return;
        }

        if !matches!(
            self.connect.draft.protocol_type(),
            ProtocolType::Via | ProtocolType::Vial
        ) {
            self.session.draft_layout_name = self.session.active_layout_name.clone();
            return;
        }

        let Some(definition) = self.session.connected_definition.as_ref() else {
            self.ui.settings_error =
                Some("Missing keyboard definition for live layout switch".to_string());
            self.session.draft_layout_name = self.session.active_layout_name.clone();
            return;
        };

        let selected_layout = self.session.draft_layout_name.clone();
        let next_layout = match definition.get_layout(&selected_layout) {
            Ok(layout) => layout,
            Err(e) => {
                self.ui.settings_error = Some(format!("Failed to switch layout: {e}"));
                self.session.draft_layout_name = self.session.active_layout_name.clone();
                return;
            }
        };

        let AppConnectionState::Connected { keyboard } = &mut self.session.connection else {
            return;
        };

        keyboard.set_layout(next_layout);
        self.session.active_layout_name = selected_layout;
    }

    pub(super) fn get_anchor_params(&self) -> (Align2, egui::Vec2) {
        match self.settings.active.position {
            WindowPosition::TopLeft => (
                Align2::LEFT_TOP,
                egui::vec2(
                    self.settings.active.margin as f32,
                    self.settings.active.margin as f32,
                ),
            ),
            WindowPosition::TopRight => (
                Align2::RIGHT_TOP,
                egui::vec2(
                    -(self.settings.active.margin as f32),
                    self.settings.active.margin as f32,
                ),
            ),
            WindowPosition::BottomLeft => (
                Align2::LEFT_BOTTOM,
                egui::vec2(
                    self.settings.active.margin as f32,
                    -(self.settings.active.margin as f32),
                ),
            ),
            WindowPosition::BottomRight => (
                Align2::RIGHT_BOTTOM,
                egui::vec2(
                    -(self.settings.active.margin as f32),
                    -(self.settings.active.margin as f32),
                ),
            ),
            WindowPosition::Bottom => (
                Align2::CENTER_BOTTOM,
                egui::vec2(0.0, -(self.settings.active.margin as f32)),
            ),
            WindowPosition::Top => (
                Align2::CENTER_TOP,
                egui::vec2(0.0, self.settings.active.margin as f32),
            ),
        }
    }

    pub(super) fn overlay_visible(&self) -> bool {
        match &self.session.connection {
            AppConnectionState::Disconnected => false,
            AppConnectionState::Connected { keyboard } => {
                if self.ui.settings_visible {
                    return true;
                }

                // Crosses 42 fork: keep the overlay up while any key has
                // been held longer than the configured threshold. This is
                // the host-side approximation of "modifier engaged" — works
                // for hml/hmr homerow mods and plain held &kp LSHIFT etc.
                if self.settings.active.show_on_key_held {
                    let threshold =
                        Duration::from_millis(self.settings.active.hold_threshold_ms as u64);
                    if keyboard.any_key_held_longer_than(threshold) {
                        return true;
                    }
                }

                match keyboard.time_to_hide_overlay.lock().unwrap().as_ref() {
                    Some(time_to_hide) => Instant::now() < *time_to_hide,
                    None => true,
                }
            }
        }
    }
}
