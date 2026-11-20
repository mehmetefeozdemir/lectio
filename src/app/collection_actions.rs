//! Phase 4b actions: collections across courses and the Home list's filter
//! and sort (ADR-026). The rules live in `lectio_core::collections`.

use lectio_core::collections::HomeFilter;

use super::*;

impl LectioApp {
    /// Handles collection actions; returns any other action unchanged.
    pub(super) fn apply_collections(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::NewCollection { name, with } => self.new_collection(&name, with),
            Action::RenameCollection(id, name) => self.change_library(|lib| lib.with_collection_renamed(id, &name)),
            Action::DeleteCollection(id) => {
                self.change_library(|lib| lib.without_collection(id));
                if self.prefs().home_filter == HomeFilter::Collection(id) {
                    self.change_prefs(|p| p.home_filter = HomeFilter::All);
                }
            }
            Action::AddToCollection { collection, session } => {
                self.change_library(|lib| lib.with_session_in_collection(collection, session));
            }
            Action::RemoveFromCollection { collection, session } => {
                self.change_library(|lib| lib.without_session_in_collection(collection, session));
            }
            Action::SetHomeFilter(filter) => self.change_prefs(|p| p.home_filter = filter),
            Action::SetHomeSort(sort) => self.change_prefs(|p| p.home_sort = sort),
            other => return Some(other),
        }
        None
    }

    fn new_collection(&mut self, name: &str, with: Option<SessionId>) {
        let made = self.library.with_collection(name).and_then(|(lib, id)| match with {
            Some(session) => lib.with_session_in_collection(id, session),
            None => Ok(lib),
        });
        match made {
            Ok(library) => {
                self.library = library;
                self.buffers.new_collection.clear();
                self.buffers.adding_collection = None;
            }
            Err(e) => self.show_error(e),
        }
    }
}
