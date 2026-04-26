#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Unchanged,
    Created,
    Updated,
    Removed,
}

impl ChangeKind {
    #[must_use]
    pub const fn changed(self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSubject {
    FsEntry,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionChange {
    pub kind: ChangeKind,
    pub subject: ChangeSubject,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<super::TargetPath>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ExecutionChange {
    #[must_use]
    pub fn fs_entry(kind: ChangeKind, path: impl Into<super::TargetPath>) -> Self {
        Self {
            kind,
            subject: ChangeSubject::FsEntry,
            path: Some(path.into()),
            detail: None,
        }
    }

    #[must_use]
    pub fn command(kind: ChangeKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            subject: ChangeSubject::Command,
            path: None,
            detail: Some(detail.into()),
        }
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub const fn changed(&self) -> bool {
        self.kind.changed()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ExecutionResult {
    pub changes: Vec<ExecutionChange>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ExecutionResult {
    #[must_use]
    pub fn unchanged() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn fs_entry(kind: ChangeKind, path: impl Into<super::TargetPath>) -> Self {
        Self {
            changes: vec![ExecutionChange::fs_entry(kind, path)],
            message: None,
            data: None,
        }
    }

    #[must_use]
    pub fn command(kind: ChangeKind, detail: impl Into<String>) -> Self {
        Self {
            changes: vec![ExecutionChange::command(kind, detail)],
            message: None,
            data: None,
        }
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    #[must_use]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        self.changes.iter().any(ExecutionChange::changed)
    }

    pub fn merge(&mut self, other: Self) {
        self.changes.extend(other.changes);
        if self.message.is_none() {
            self.message = other.message;
        }
        if self.data.is_none() {
            self.data = other.data;
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(default)]
pub struct ValidationResult {
    pub ok: bool,
    pub message: Option<String>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self {
            ok: true,
            message: None,
        }
    }
}
