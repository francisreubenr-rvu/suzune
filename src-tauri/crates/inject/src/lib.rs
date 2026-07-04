//! suzune-inject: platform-neutral public API for injecting transcribed text
//! into the focused UI element of whatever application the user is dictating
//! into.
//!
//! Three methods, in order of preference for the automatic fallback chain:
//!
//! 1. [`InjectionMethod::AxInsert`] — write directly into the focused
//!    accessibility element via the macOS Accessibility API. Never touches
//!    the system clipboard.
//! 2. [`InjectionMethod::ClipboardPaste`] — save the clipboard, write the
//!    text, synthesize Cmd+V, restore the clipboard.
//! 3. [`InjectionMethod::DirectType`] — simulate individual keystrokes.
//!    Available on request but intentionally excluded from the automatic
//!    chain (slow, and can trigger autocomplete/autocorrect side effects).
//!
//! [`inject_auto`] uses ClipboardPaste as the primary method (reliable in
//! terminals and Electron apps) and falls back to AxInsert; use
//! [`inject_auto_with_primary`] to prefer the write-only AX path instead.

#[cfg(target_os = "macos")]
mod macos;

mod clipboard;
mod direct_type;
mod error;

pub use error::InjectError;

/// A method of injecting text into the focused UI element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectionMethod {
    /// Write-only insert via the macOS Accessibility API. Does not use the
    /// clipboard. macOS only; on other platforms this method always fails
    /// with [`InjectError::Unsupported`].
    AxInsert,
    /// Save clipboard, write text, synthesize Cmd+V / Ctrl+V, restore
    /// clipboard.
    ClipboardPaste,
    /// Simulate individual keystrokes for the given text. Not part of the
    /// automatic fallback chain — callers opt into this explicitly.
    DirectType,
}

impl InjectionMethod {
    /// Parse a settings string ("clipboard", "ax", "type") into a method,
    /// defaulting to the reliable clipboard-paste for unknown/empty input.
    pub fn from_setting(s: &str) -> InjectionMethod {
        match s.trim().to_lowercase().as_str() {
            "ax" | "ax-insert" | "accessibility" => InjectionMethod::AxInsert,
            "type" | "direct" | "direct-type" | "keystrokes" => InjectionMethod::DirectType,
            _ => InjectionMethod::ClipboardPaste,
        }
    }
}

impl std::fmt::Display for InjectionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            InjectionMethod::AxInsert => "ax-insert",
            InjectionMethod::ClipboardPaste => "clipboard-paste",
            InjectionMethod::DirectType => "direct-type",
        };
        f.write_str(s)
    }
}

/// Inject `text` using the specified `method`.
///
/// Unlike [`inject_auto`], this does not fall back to another method on
/// failure — the caller asked for a specific method and gets a specific
/// result.
pub fn inject(text: &str, method: InjectionMethod) -> Result<(), InjectError> {
    match method {
        InjectionMethod::AxInsert => ax_insert(text),
        InjectionMethod::ClipboardPaste => clipboard::paste(text),
        InjectionMethod::DirectType => direct_type::type_text(text),
    }
}

/// Inject `text` using the automatic fallback chain with clipboard-paste as
/// the primary method (see [`inject_auto_with_primary`]).
///
/// Clipboard-paste is the reliable default: it works in every app that
/// accepts Cmd+V, including terminals and Electron apps (WhatsApp, Slack,
/// VS Code) where AX-insert silently no-ops — those apps' focused element
/// accepts an `AXSelectedText` write and returns success without actually
/// inserting anything, so an AX-first chain would wrongly report success
/// and never fall back.
pub fn inject_auto(text: &str) -> Result<InjectionMethod, InjectError> {
    inject_auto_with_primary(text, InjectionMethod::ClipboardPaste)
}

/// Inject `text`, trying `primary` first and falling back to the other of
/// {AX-insert, clipboard-paste} if it fails. Returns the method that
/// actually succeeded.
///
/// `primary = AxInsert` keeps the clipboard untouched when it works, at the
/// cost of failing (invisibly-succeeding) in terminals/Electron apps.
/// `primary = ClipboardPaste` is the reliable default. `DirectType` is not
/// part of the chain — see module docs; if passed as `primary` it is used
/// with no fallback.
pub fn inject_auto_with_primary(
    text: &str,
    primary: InjectionMethod,
) -> Result<InjectionMethod, InjectError> {
    let (fallback, primary_call, fallback_call): (
        InjectionMethod,
        fn(&str) -> Result<(), InjectError>,
        fn(&str) -> Result<(), InjectError>,
    ) = match primary {
        InjectionMethod::ClipboardPaste => {
            (InjectionMethod::AxInsert, clipboard::paste, ax_insert)
        }
        InjectionMethod::AxInsert => {
            (InjectionMethod::ClipboardPaste, ax_insert, clipboard::paste)
        }
        // Explicit direct-type request: honor it with no fallback.
        InjectionMethod::DirectType => {
            direct_type::type_text(text)?;
            log::info!("suzune-inject: injected via {}", InjectionMethod::DirectType);
            return Ok(InjectionMethod::DirectType);
        }
    };

    match primary_call(text) {
        Ok(()) => {
            log::info!("suzune-inject: injected via {primary}");
            Ok(primary)
        }
        Err(primary_err) => {
            log::info!(
                "suzune-inject: {primary} unavailable ({primary_err}), falling back to {fallback}"
            );
            fallback_call(text)?;
            log::info!("suzune-inject: injected via {fallback}");
            Ok(fallback)
        }
    }
}

#[cfg(target_os = "macos")]
fn ax_insert(text: &str) -> Result<(), InjectError> {
    macos::ax_insert(text)
}

#[cfg(not(target_os = "macos"))]
fn ax_insert(_text: &str) -> Result<(), InjectError> {
    Err(InjectError::Unsupported(
        "AxInsert is only implemented on macOS",
    ))
}
