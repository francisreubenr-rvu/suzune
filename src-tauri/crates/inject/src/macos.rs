//! macOS Accessibility API (AX) write-only text insertion.
//!
//! Strategy: resolve the system-wide focused UI element
//! (`AXUIElementCreateSystemWide` + `kAXFocusedUIElementAttribute`), then set
//! `kAXSelectedTextAttribute` on it to `text`. When the element's current
//! selection is collapsed (the common case — a caret with no selection),
//! setting `AXSelectedText` replaces the (empty) selection with `text`,
//! which is exactly an insert-at-caret. This never touches the system
//! clipboard.
//!
//! If the focused element does not expose a settable `AXSelectedText`
//! (many custom-drawn text views don't), the AX API returns a typed error
//! and the caller (`inject_auto`) falls back to clipboard-paste.

use crate::InjectError;
use accessibility_sys::{
    kAXErrorAPIDisabled, kAXErrorSuccess, kAXFocusedUIElementAttribute,
    kAXSelectedTextAttribute, AXError, AXIsProcessTrusted, AXUIElementCopyAttributeValue,
    AXUIElementCreateSystemWide, AXUIElementRef, AXUIElementSetAttributeValue,
};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::CFString;
use std::ffi::c_void;

/// RAII wrapper that releases a Core Foundation `AXUIElementRef` on drop.
struct AxElement(AXUIElementRef);

impl Drop for AxElement {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0 as CFTypeRef) };
        }
    }
}

pub(crate) fn ax_insert(text: &str) -> Result<(), InjectError> {
    // AXIsProcessTrusted() never prompts; it only reports current status.
    // whispr-inject never auto-prompts for accessibility permission.
    if !unsafe { AXIsProcessTrusted() } {
        return Err(InjectError::AccessibilityPermissionDenied);
    }

    let system_wide = unsafe { AXUIElementCreateSystemWide() };
    if system_wide.is_null() {
        return Err(InjectError::NoFocusedElement);
    }
    let system_wide = AxElement(system_wide);

    let focused_attr = CFString::new(kAXFocusedUIElementAttribute);
    let mut focused_ref: CFTypeRef = std::ptr::null();
    let err: AXError = unsafe {
        AXUIElementCopyAttributeValue(
            system_wide.0,
            focused_attr.as_concrete_TypeRef(),
            &mut focused_ref,
        )
    };
    if err != kAXErrorSuccess {
        return Err(map_focus_error(err));
    }
    if focused_ref.is_null() {
        return Err(InjectError::NoFocusedElement);
    }
    let focused = AxElement(focused_ref as *mut c_void as AXUIElementRef);

    let selected_text_attr = CFString::new(kAXSelectedTextAttribute);
    let value = CFString::new(text);
    let err: AXError = unsafe {
        AXUIElementSetAttributeValue(
            focused.0,
            selected_text_attr.as_concrete_TypeRef(),
            value.as_concrete_TypeRef() as CFTypeRef,
        )
    };

    if err == kAXErrorSuccess {
        Ok(())
    } else {
        Err(map_write_error(err))
    }
}

/// Maps an `AXError` from resolving the focused element to a typed
/// `InjectError`.
fn map_focus_error(err: AXError) -> InjectError {
    if err == kAXErrorAPIDisabled {
        InjectError::AccessibilityPermissionDenied
    } else {
        InjectError::NoFocusedElement
    }
}

/// Maps an `AXError` from the `AXSelectedText` write attempt to a typed
/// `InjectError`.
fn map_write_error(err: AXError) -> InjectError {
    if err == kAXErrorAPIDisabled {
        InjectError::AccessibilityPermissionDenied
    } else {
        InjectError::ElementRejectedWrite
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accessibility_sys::{
        kAXErrorAttributeUnsupported, kAXErrorCannotComplete, kAXErrorFailure,
        kAXErrorIllegalArgument, kAXErrorNoValue,
    };

    #[test]
    fn api_disabled_maps_to_permission_denied_for_focus() {
        assert!(matches!(
            map_focus_error(kAXErrorAPIDisabled),
            InjectError::AccessibilityPermissionDenied
        ));
    }

    #[test]
    fn other_focus_errors_map_to_no_focused_element() {
        assert!(matches!(
            map_focus_error(kAXErrorNoValue),
            InjectError::NoFocusedElement
        ));
        assert!(matches!(
            map_focus_error(kAXErrorCannotComplete),
            InjectError::NoFocusedElement
        ));
    }

    #[test]
    fn api_disabled_maps_to_permission_denied_for_write() {
        assert!(matches!(
            map_write_error(kAXErrorAPIDisabled),
            InjectError::AccessibilityPermissionDenied
        ));
    }

    #[test]
    fn other_write_errors_map_to_element_rejected() {
        assert!(matches!(
            map_write_error(kAXErrorAttributeUnsupported),
            InjectError::ElementRejectedWrite
        ));
        assert!(matches!(
            map_write_error(kAXErrorIllegalArgument),
            InjectError::ElementRejectedWrite
        ));
        assert!(matches!(
            map_write_error(kAXErrorFailure),
            InjectError::ElementRejectedWrite
        ));
    }
}
