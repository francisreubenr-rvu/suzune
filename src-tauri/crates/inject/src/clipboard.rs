//! Clipboard-paste injection: save the current clipboard, write the text,
//! synthesize Cmd+V (macOS) / Ctrl+V (Windows/Linux) with a layout-
//! independent virtual keycode, then restore the previous clipboard
//! contents after a short delay.
//!
//! If the clipboard did not hold plain text before we wrote to it (e.g. an
//! image or file list), we do not attempt to save or restore anything —
//! silently leaving the new text on the clipboard is preferable to
//! discarding non-text content we cannot faithfully round-trip.

use crate::InjectError;
use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

/// Delay after writing to the clipboard, before sending the paste
/// keystroke, so the target application's clipboard-change notification
/// has time to fire.
const WRITE_SETTLE_MS: u64 = 50;
/// Delay after the paste keystroke completes, before restoring the
/// original clipboard, so the target app finishes reading the pasted
/// value.
const PASTE_SETTLE_MS: u64 = 50;

pub(crate) fn paste(text: &str) -> Result<(), InjectError> {
    let mut clipboard =
        Clipboard::new().map_err(|e| InjectError::Clipboard(format!("failed to open clipboard: {e}")))?;

    // Only plain text is saved/restored. If the clipboard held something
    // else (or nothing), `previous` is `None` and restore is a no-op.
    let previous = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_owned())
        .map_err(|e| InjectError::Clipboard(format!("failed to write clipboard: {e}")))?;

    std::thread::sleep(Duration::from_millis(WRITE_SETTLE_MS));

    send_paste_keystroke().map_err(InjectError::Keyboard)?;

    std::thread::sleep(Duration::from_millis(PASTE_SETTLE_MS));

    if let Some(previous) = previous {
        // Best-effort restore: a failure here must not surface as an
        // injection failure, since the paste itself already succeeded.
        let _ = clipboard.set_text(previous);
    }

    Ok(())
}

/// Synthesizes a paste keystroke using platform-specific virtual key codes
/// so it works regardless of the active keyboard layout (matches the
/// approach used by Handy's `input::send_paste_ctrl_v`).
fn send_paste_keystroke() -> Result<(), String> {
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("failed to initialize enigo: {e}"))?;

    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9)); // kVK_ANSI_V
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    enigo
        .key(modifier_key, Direction::Press)
        .map_err(|e| format!("failed to press modifier key: {e}"))?;
    enigo
        .key(v_key_code, Direction::Click)
        .map_err(|e| format!("failed to click V key: {e}"))?;

    std::thread::sleep(Duration::from_millis(100));

    enigo
        .key(modifier_key, Direction::Release)
        .map_err(|e| format!("failed to release modifier key: {e}"))?;

    Ok(())
}
