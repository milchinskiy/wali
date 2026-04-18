#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Unchanged,
    Created,
    Updated,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeResult {
    pub kind: ChangeKind,
}

impl ChangeResult {
    pub const UNCHANGED: Self = Self {
        kind: ChangeKind::Unchanged,
    };

    pub const CREATED: Self = Self {
        kind: ChangeKind::Created,
    };

    pub const UPDATED: Self = Self {
        kind: ChangeKind::Updated,
    };

    pub const REMOVED: Self = Self {
        kind: ChangeKind::Removed,
    };

    #[must_use]
    pub fn changed(self) -> bool {
        !matches!(self.kind, ChangeKind::Unchanged)
    }
}

impl serde::Serialize for ChangeResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("ChangeResult", 2)?;
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("changed", &self.changed())?;
        state.end()
    }
}
