//! Crosses 42 hardcoded label overrides.
//!
//! Upstream KeyPeek decodes ZMK Studio's well-known behaviors (KeyPress,
//! MomentaryLayer, ModTap, etc.) into nice keycap labels. Anything custom
//! defined in a keymap's `behaviors { }` block falls through to
//! `Behavior::Unknown { behavior_id, param1, param2 }` and renders as a raw
//! "0x1B"-style hex string.
//!
//! This fork hardcodes mappings for the custom behaviors used by the
//! Crosses 42 keymap (`max@rootevidence.com`'s personal split keyboard).
//! See https://github.com/Good-Great-Grand-Wonderful/crosses for the
//! firmware repo this is paired with.
//!
//! ## Behavior IDs are not stable across keymap edits
//!
//! ZMK Studio assigns numeric IDs to custom behaviors at firmware-build
//! time. As long as the `behaviors { }` block in `crosses.keymap` doesn't
//! change shape, these IDs hold. If you add, remove, or reorder behaviors,
//! KeyPeek will start showing fresh `0x..` strings — read them off the
//! overlay and update the table below.
//!
//! ⚠ IDs below are STALE after Stage 9 keymap edits (2026-05-12).
//! mcs and mdc removed (BASE pos 23/32/33 now plain &kp); msc added
//! (new ;: mod-morph at BASE pos 35). Behaviors-block shape changed →
//! every custom-behavior ID needs to be re-read from the overlay after
//! rebuilding firmware.
//!
//! ## Mappings (IDs need re-read after Stage 9 build)
//!
//! | ID    | DTS name   | Behavior class              | Display strategy |
//! |-------|------------|-----------------------------|------------------|
//! | ??    | hml        | hold-tap (MOD / KEY) ×4     | tap-side via HidUsage |
//! | ??    | hmr        | hold-tap (MOD / KEY) ×4     | tap-side via HidUsage |
//! | ??    | ht         | hold-tap (TILDE / ESC)      | tap-side via HidUsage |
//! | ??    | sht        | hold-tap (LSHFT / SPACE)    | tap-side via HidUsage |
//! | ??    | bspc_del   | mod-morph (BSPC ↔ DEL)      | static "Bs/Del"  |
//! | ??    | msc        | mod-morph (; ↔ :)           | static ";:"      |
//! | ??    | td_sym     | tap-dance (mo SYM / numword)| static "Sym"     |
//! | ??    | swapper    | tri-state (Alt-Tab)         | static "AltTab"  |

use crate::layout_key::{KeycodeKind, Label, LayoutKey};
use zmk_studio_api::HidUsage;

use super::hid_usage::hid_usage_to_layout_key;

// ⚠ IDs are STALE — re-read after Stage 9 firmware build.
// Order in behaviors block (post Stage 9):
//   hml, hmr, ht, sht, bspc_del, msc, td_sym, swapper
// (mcs and mdc were removed; msc added in mcs's old slot.)
// TODO: After building + running, read fresh IDs from overlay and update.
const HML: i32 = 0x1B;       // TODO: re-read after Stage 9 build
const HMR: i32 = 0x1C;       // TODO: re-read after Stage 9 build
const HT: i32 = 0x1A;        // TODO: re-read after Stage 9 build
const SHT: i32 = 0x26;       // TODO: re-read after Stage 9 build (was confirmed 2026-05-10)
const BSPC_DEL: i32 = 0x19;  // TODO: re-read after Stage 9 build
const MSC: i32 = 0x24;       // TODO: re-read after Stage 9 build (placeholder ID)
const TD_SYM: i32 = 0x21;    // TODO: re-read after Stage 9 build
const SWAPPER: i32 = 0x27;   // TODO: re-read after Stage 9 build (was confirmed 2026-05-10)

/// If `behavior_id` is one of the Crosses 42 custom behaviors, return a
/// rendered `LayoutKey` for it. Otherwise return `None` so the caller can
/// fall through to the upstream hex-string fallback.
///
/// `param1` and `param2` are the raw u32s from `Behavior::Unknown`. For
/// hold-tap behaviors (hml/hmr/ht) ZMK encodes them as HID usages — same
/// layout as `Behavior::ModTap`'s `hold` and `tap` fields.
pub fn crosses42_layout_key(behavior_id: i32, param1: u32, param2: u32) -> Option<LayoutKey> {
    match behavior_id {
        // Hold-taps. Mirrors the upstream `Behavior::ModTap` arm in
        // behavior.rs: tap label is primary, hold label is secondary,
        // styled as Modifier so the overlay colors it consistently.
        // SHT (Space/Shift) is treated identically — shows "SPC" with
        // "⇧" as hold annotation.
        HT | HML | HMR | SHT => {
            let hold_key = hid_usage_to_layout_key(HidUsage::from_encoded(param1));
            let tap_key = hid_usage_to_layout_key(HidUsage::from_encoded(param2));
            Some(LayoutKey {
                tap: tap_key.tap,
                hold: Some(hold_key.tap),
                symbol: tap_key.symbol,
                kind: KeycodeKind::Modifier,
                layer_ref: None,
            })
        }

        // Mod-morphs: show BOTH variants so the Shift behavior is visible.
        // ";:" means tap = semicolon, Shift+tap = colon (Stage 9).
        BSPC_DEL => Some(special("Bs/Del")),
        MSC => Some(basic(";:")),

        // Tap-dances and tri-states.
        TD_SYM => Some(special("Sym")),
        SWAPPER => Some(special("AltTab")),

        _ => None,
    }
}

fn basic(label: &str) -> LayoutKey {
    LayoutKey {
        tap: Label::new(label),
        ..Default::default()
    }
}

fn special(label: &str) -> LayoutKey {
    LayoutKey {
        tap: Label::new(label),
        kind: KeycodeKind::Special,
        ..Default::default()
    }
}
