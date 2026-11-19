//! Picking a file in the browser (ADR-013). `rfd` shows an
//! `<input type="file">`; reading the chosen file is asynchronous, so the
//! result arrives in a `Pending` on a later frame.
//!
//! Browsers only open a file chooser right after a click ("user
//! activation"). rfd checks that, and if the click is too long ago it shows a
//! small "choose file" box the user can click instead.

use crate::platform::{FileKind, Pending, PickedFile};

pub fn pick(kind: FileKind) -> Pending<Option<PickedFile>> {
    let pending = Pending::waiting();
    let done = pending.clone();
    let (name, extensions) = kind.filter();
    let dialog = rfd::AsyncFileDialog::new().add_filter(name, extensions);
    wasm_bindgen_futures::spawn_local(async move {
        let picked = match dialog.pick_file().await {
            Some(handle) => Some(PickedFile { name: handle.file_name(), bytes: handle.read().await }),
            None => None,
        };
        done.complete(Ok(picked));
    });
    pending
}
