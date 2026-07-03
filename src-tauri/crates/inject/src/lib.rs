//! whispr-inject: platform-neutral public API for injecting transcribed text
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
//! [`inject_auto`] tries AxInsert, then falls back to ClipboardPaste, and
//! reports which method actually succeeded.

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

/// Inject `text` using the automatic fallback chain: try AX-insert first
/// (write-only, no clipboard exposure); if that fails for any reason, fall
/// back to clipboard-paste. Returns the method that succeeded.
///
/// `DirectType` is deliberately not part of this chain — see module docs.
pub fn inject_auto(text: &str) -> Result<InjectionMethod, InjectError> {
    match ax_insert(text) {
        Ok(()) => {
            log::info!("whispr-inject: injected via {}", InjectionMethod::AxInsert);
            Ok(InjectionMethod::AxInsert)
        }
        Err(ax_err) => {
            log::info!(
                "whispr-inject: AxInsert unavailable ({ax_err}), falling back to {}",
                InjectionMethod::ClipboardPaste
            );
            clipboard::paste(text)?;
            log::info!(
                "whispr-inject: injected via {}",
                InjectionMethod::ClipboardPaste
            );
            Ok(InjectionMethod::ClipboardPaste)
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
