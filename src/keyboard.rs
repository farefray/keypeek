use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::key_matrix::KeyMatrix;
use crate::layout_key::LayoutKey;
use crate::protocols::{KeyboardLayout, KeyboardProtocol};
use crate::ui_wake::UiWake;

/// Snapshot of the user's visibility preferences shared with the HID-reader
/// thread. Mutated atomically from the UI thread when settings change so the
/// reader thread always sees the latest config.
///
/// Today only `show_on_non_base_layer` is consumed inside the reader thread
/// (it gates whether layer-state events keep the overlay alive). The other
/// two fields are read on the UI side via `Settings::*` directly, but kept
/// here so the visibility-options model stays whole — moving any of those
/// checks into the HID thread later is then a one-line change.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct VisibilityOptions {
    pub show_on_non_base_layer: bool,
    pub show_on_key_held: bool,
    pub hold_threshold_ms: u32,
}

impl Default for VisibilityOptions {
    fn default() -> Self {
        Self {
            show_on_non_base_layer: true,
            show_on_key_held: true,
            hold_threshold_ms: 200,
        }
    }
}

pub struct Keyboard {
    pub layout: KeyboardLayout,
    pub time_to_hide_overlay: Arc<Mutex<Option<Instant>>>,
    matrix: Arc<Mutex<KeyMatrix>>,
    layer_state: Arc<Mutex<u32>>,
    default_layer_state: Arc<Mutex<u32>>,
    timeout_ms: Arc<Mutex<i64>>,
    visibility: Arc<Mutex<VisibilityOptions>>,
    /// Map of (row, col) -> press start `Instant` for every key currently
    /// held down. Used by the host-side modifier-detection feature: any
    /// entry older than `hold_threshold_ms` keeps the overlay visible.
    pressed_since: Arc<Mutex<HashMap<(usize, usize), Instant>>>,
}

impl Keyboard {
    pub fn new(
        protocol: Box<dyn KeyboardProtocol>,
        layout_name: String,
        timeout: i64,
        visibility: VisibilityOptions,
        ui_wake: UiWake,
    ) -> Result<Self, String> {
        let definition = protocol.get_layout_definition();

        let layout = definition
            .get_layout(&layout_name)
            .map_err(|_| "Failed to get layout".to_string())?;

        let layers = protocol
            .get_layer_count()
            .map_err(|e| format!("Failed to get layer count: {e}"))?;

        let keys = protocol.read_all_keys(layers, definition.rows, definition.cols);
        let matrix = KeyMatrix::from_layout_keys(keys, definition.rows, definition.cols);

        let layer_state = Arc::new(Mutex::new(0));
        let default_layer_state = Arc::new(Mutex::new(0));
        let time_to_hide_overlay = Arc::new(Mutex::new(Some(Instant::now())));
        let timeout_ms = Arc::new(Mutex::new(timeout));
        let visibility = Arc::new(Mutex::new(visibility));
        let pressed_since = Arc::new(Mutex::new(HashMap::new()));
        let matrix = Arc::new(Mutex::new(matrix));

        let keyboard = Keyboard {
            layout,
            matrix: Arc::clone(&matrix),
            time_to_hide_overlay: Arc::clone(&time_to_hide_overlay),
            layer_state: Arc::clone(&layer_state),
            default_layer_state: Arc::clone(&default_layer_state),
            timeout_ms: Arc::clone(&timeout_ms),
            visibility: Arc::clone(&visibility),
            pressed_since: Arc::clone(&pressed_since),
        };

        let layer_state_clone = Arc::clone(&keyboard.layer_state);
        let default_layer_state_clone = Arc::clone(&keyboard.default_layer_state);
        let time_to_hide_clone = Arc::clone(&keyboard.time_to_hide_overlay);
        let timeout_clone = Arc::clone(&keyboard.timeout_ms);
        let visibility_clone = Arc::clone(&keyboard.visibility);
        let pressed_since_clone = Arc::clone(&keyboard.pressed_since);
        let matrix_clone = Arc::clone(&matrix);

        thread::spawn(move || loop {
            if let Ok(response) = protocol.hid_read() {
                let mut needs_repaint = false;
                if response[0] == 0xff {
                    let size = response[1] as usize;

                    let mut default_bytes = [0u8; 4];
                    default_bytes[..size].copy_from_slice(&response[2..2 + size]);
                    let default_layer_state = u32::from_le_bytes(default_bytes);

                    let mut layer_bytes = [0u8; 4];
                    layer_bytes[..size].copy_from_slice(&response[2 + size..2 + 2 * size]);
                    let layer_state = u32::from_le_bytes(layer_bytes);

                    let vis = *visibility_clone.lock().unwrap();
                    if vis.show_on_non_base_layer && layer_state > 1 {
                        *time_to_hide_clone.lock().unwrap() = None;
                    } else {
                        let timeout = *timeout_clone.lock().unwrap();
                        if timeout < 0 {
                            *time_to_hide_clone.lock().unwrap() = None;
                        } else {
                            let time_to_hide =
                                Instant::now() + Duration::from_millis(timeout as u64);
                            *time_to_hide_clone.lock().unwrap() = Some(time_to_hide);
                        }
                    }

                    *layer_state_clone.lock().unwrap() = layer_state;
                    *default_layer_state_clone.lock().unwrap() = default_layer_state;
                    needs_repaint = true;
                } else if response[0] == 0xF1 {
                    let row = response[1] as usize;
                    let col = response[2] as usize;
                    let pressed = response[3];
                    if let Ok(mut mat) = matrix_clone.lock() {
                        mat.set_pressed(row, col, pressed != 0);
                    }
                    if let Ok(mut held) = pressed_since_clone.lock() {
                        if pressed != 0 {
                            held.insert((row, col), Instant::now());
                        } else {
                            held.remove(&(row, col));
                        }
                    }
                    // Repaint on press/release so the held-key visibility
                    // gate gets re-evaluated; the UI side schedules another
                    // wake at threshold-crossing.
                    needs_repaint = true;
                }

                if needs_repaint {
                    ui_wake.request_repaint();
                }
            }
        });

        Ok(keyboard)
    }

    /// Replace the visibility options seen by the HID-reader thread on its
    /// next event. Cheap (atomic Mutex swap); safe to call from the UI
    /// thread on every settings change.
    pub fn set_visibility_options(&self, opts: VisibilityOptions) {
        *self.visibility.lock().unwrap() = opts;
    }

    /// True if any matrix key has been continuously held for at least
    /// `threshold`. Used by the host-side modifier-detection visibility
    /// gate.
    pub fn any_key_held_longer_than(&self, threshold: Duration) -> bool {
        let now = Instant::now();
        self.pressed_since
            .lock()
            .unwrap()
            .values()
            .any(|since| now.saturating_duration_since(*since) >= threshold)
    }

    /// `Instant` of the oldest currently-held key, if any. Used by the UI
    /// to schedule a repaint right at the moment the held-key threshold
    /// would flip the overlay on (so the visibility transition isn't
    /// gated on the next unrelated repaint).
    pub fn earliest_held_key_press_time(&self) -> Option<Instant> {
        self.pressed_since
            .lock()
            .unwrap()
            .values()
            .copied()
            .min()
    }

    pub fn get_effective_key_layer(&self, row: usize, col: usize) -> (u8, bool) {
        let layer_state = *self.layer_state.lock().unwrap();
        let default_layer_state = *self.default_layer_state.lock().unwrap();
        let matrix = self.matrix.lock().unwrap();
        let num_layers = matrix.get_num_layers().min(32);

        // Track if there is any active momentary layer above the effective layer
        // (i.e, key should be shown as background key)
        let mut active_layer_above = false;

        for i in (1..num_layers).rev() {
            let layer_mask = 1u32 << (i as u32);
            let is_active_default_layer = (default_layer_state & layer_mask) != 0;
            let is_active_momentary_layer = (layer_state & layer_mask) != 0;
            if is_active_momentary_layer || is_active_default_layer {
                if !matrix.is_transparent(i, row, col) {
                    return (i as u8, is_active_default_layer && active_layer_above);
                }
            }
            active_layer_above |= is_active_momentary_layer;
        }

        (0, active_layer_above)
    }

    pub fn get_key(&self, layer: usize, row: usize, col: usize) -> Option<LayoutKey> {
        self.matrix
            .lock()
            .unwrap()
            .get_key(layer, row, col)
            .cloned()
    }

    pub fn is_key_pressed(&self, row: usize, col: usize) -> bool {
        self.matrix.lock().unwrap().is_pressed(row, col)
    }

    pub fn set_timeout(&self, timeout: i64) {
        *self.timeout_ms.lock().unwrap() = timeout;
    }

    pub fn set_layout(&mut self, layout: KeyboardLayout) {
        self.layout = layout;
    }
}
