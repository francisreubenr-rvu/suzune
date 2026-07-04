use std::fmt;

/// Errors produced by injection attempts.
///
/// This is deliberately a plain enum (not `anyhow::Error`) so callers
/// implementing the fallback chain can match on *why* a method failed
/// rather than just knowing that it did.
#[derive(Debug)]
pub enum InjectError {
    /// The macOS Accessibility permission has not been granted to this
    /// process. The caller should tell the user to grant it in
    /// System Settings > Privacy & Security > Accessibility, then retry.
    /// fude-inject never auto-prompts for this permission.
    AccessibilityPermissionDenied,
    /// There is no focused UI element to write into, or the system-wide
    /// focused element could not be resolved.
    NoFocusedElement,
    /// The focused element does not support the accessibility attribute
    /// this method needs (e.g. it is not a text field, or it exposes
    /// `kAXSelectedTextAttribute` as read-only).
    ElementRejectedWrite,
    /// The requested method is not implemented on the current platform.
    Unsupported(&'static str),
    /// The system clipboard could not be read or written.
    Clipboard(String),
    /// Synthesizing keyboard input failed.
    Keyboard(String),
}

impl fmt::Display for InjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InjectError::AccessibilityPermissionDenied => write!(
                f,
                "Accessibility permission not granted. Open System Settings > \
                 Privacy & Security > Accessibility and enable it for this app, \
                 then try again."
            ),
            InjectError::NoFocusedElement => {
                write!(f, "no focused UI element to insert text into")
            }
            InjectError::ElementRejectedWrite => write!(
                f,
                "the focused UI element rejected the accessibility write"
            ),
            InjectError::Unsupported(msg) => write!(f, "unsupported: {msg}"),
            InjectError::Clipboard(msg) => write!(f, "clipboard error: {msg}"),
            InjectError::Keyboard(msg) => write!(f, "keyboard synthesis error: {msg}"),
        }
    }
}

impl std::error::Error for InjectError {}
