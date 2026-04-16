#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

