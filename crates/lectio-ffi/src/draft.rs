//! The note being typed. Its moment is when typing *started* (when the
//! student heard the thing worth writing down), the desktop's rule from
//! `lectio_core::clock::NoteDraft`.

use std::sync::{Arc, Mutex};

use lectio_core::clock::NoteDraft as CoreDraft;

#[derive(uniffi::Object, Default)]
pub struct NoteDraft {
    draft: Mutex<CoreDraft>,
}

#[uniffi::export]
impl NoteDraft {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Call on every edit with the recording's current moment; the first
    /// non-blank text starts the clock, clearing the text resets it.
    pub fn set_text(&self, text: String, now_ms: u64) {
        if let Ok(mut draft) = self.draft.lock() {
            *draft = draft.with_text(&text, now_ms);
        }
    }

    /// The moment to save the note at (`now_ms` if nothing was typed).
    pub fn anchor_ms(&self, now_ms: u64) -> u64 {
        self.draft.lock().map_or(now_ms, |draft| draft.anchor_ms(now_ms))
    }
}
