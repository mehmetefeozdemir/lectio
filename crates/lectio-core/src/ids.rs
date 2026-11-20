//! Typed ids. A `NoteId` can't be passed where a `SessionId` is expected:
//! the compiler catches the mix-up instead of a test.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new_random() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }

        /// The mobile apps pass ids as text (ADR-029).
        impl std::str::FromStr for $name {
            type Err = crate::ModelError;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(text.trim()).map(Self).map_err(|_| crate::ModelError::InvalidId)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_read_back_from_their_text() {
        let id = SessionId::new_random();
        assert_eq!(id.to_string().parse::<SessionId>(), Ok(id));
        assert_eq!("not-an-id".parse::<SessionId>(), Err(crate::ModelError::InvalidId));
    }
}

id_type!(CourseId);
id_type!(SessionId);
id_type!(NoteId);
id_type!(DeckId);
id_type!(CardId);
id_type!(CollectionId);
