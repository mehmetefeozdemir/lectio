//! The four one-tap labels. Each has a fixed identity (`LabelKind`) and a
//! display name the student can change, so renaming "Task" to "To do" never
//! breaks the Phase 4 task list, which looks labels up by kind.

use serde::{Deserialize, Serialize};

use crate::{MAX_NAME_CHARS, ModelError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelKind {
    Important,
    Review,
    Task,
    Definition,
    /// A note shown as a section title in the feed and the reading view.
    Heading,
}

impl LabelKind {
    pub const ALL: [LabelKind; 5] =
        [LabelKind::Important, LabelKind::Review, LabelKind::Task, LabelKind::Definition, LabelKind::Heading];

    pub fn default_name(self) -> &'static str {
        match self {
            Self::Important => "Important",
            Self::Review => "Review",
            Self::Task => "Task",
            Self::Definition => "Definition",
            Self::Heading => "Heading",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub kind: LabelKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelSet {
    labels: Vec<Label>,
}

impl Default for LabelSet {
    fn default() -> Self {
        let labels = LabelKind::ALL
            .iter()
            .map(|&kind| Label { kind, name: kind.default_name().to_owned() })
            .collect();
        Self { labels }
    }
}

impl LabelSet {
    pub fn iter(&self) -> impl Iterator<Item = &Label> {
        self.labels.iter()
    }

    /// Display name for a label. Falls back to the default name if a saved
    /// file is missing one, so old or hand-edited files still load.
    pub fn name(&self, kind: LabelKind) -> &str {
        self.labels
            .iter()
            .find(|l| l.kind == kind)
            .map(|l| l.name.as_str())
            .unwrap_or(kind.default_name())
    }

    pub fn renamed(&self, kind: LabelKind, new_name: &str) -> Result<LabelSet, ModelError> {
        let name = validated_name(new_name)?;
        let labels = LabelKind::ALL
            .iter()
            .map(|&k| {
                let label_name = if k == kind { name.clone() } else { self.name(k).to_owned() };
                Label { kind: k, name: label_name }
            })
            .collect();
        Ok(LabelSet { labels })
    }
}

/// Trims a user-typed name and checks it is not blank or too long.
pub(crate) fn validated_name(raw: &str) -> Result<String, ModelError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ModelError::EmptyName);
    }
    if trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(ModelError::NameTooLong { max: MAX_NAME_CHARS });
    }
    Ok(trimmed.to_owned())
}
