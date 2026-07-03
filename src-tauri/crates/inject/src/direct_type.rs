//! Direct-type injection: simulate individual keystrokes for `text` via
//! enigo. Not part of the `inject_auto` fallback chain — it is slow for
//! long dictations and can trigger autocomplete/autocorrect side effects in
//! some apps, so callers opt in explicitly via `inject(text,
//! InjectionMethod::DirectType)`.

use crate::InjectError;
use enigo::{Enigo, Keyboard, Settings};

pub(crate) fn type_text(text: &str) -> Result<(), InjectError> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| InjectError::Keyboard(format!("failed to initialize enigo: {e}")))?;

    enigo
        .text(text)
        .map_err(|e| InjectError::Keyboard(format!("failed to type text: {e}")))?;

    Ok(())
}
