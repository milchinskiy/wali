/// Kind of state transition reported by an executor or module.
///
/// `Unchanged` is part of the result contract, not absence of a result. A module
/// may report explicit unchanged entries when that helps explain why no mutation
/// was needed.
///
/// ```rust
/// use wali::executor::ChangeKind;
///
/// assert!(!ChangeKind::Unchanged.changed());
/// assert!(ChangeKind::Created.changed());
/// assert!(ChangeKind::Updated.changed());
/// assert!(ChangeKind::Removed.changed());
/// ```
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
#[serde(deny_unknown_fields)]
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

/// Structured result returned by task application.
///
/// A result is considered changed when at least one contained change has a
/// non-`unchanged` kind. Optional `data` is intended for machine-readable report
/// details such as walk output or tree operation summaries.
///
/// ```rust
/// use wali::executor::{ChangeKind, ExecutionResult};
///
/// let unchanged = ExecutionResult::unchanged();
/// assert!(!unchanged.changed());
///
/// let changed = ExecutionResult::fs_entry(ChangeKind::Created, "/tmp/wali-example");
/// assert!(changed.changed());
/// ```
#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
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

    /// Normalize and validate the apply-result contract at the Rust boundary.
    ///
    /// This is intentionally strict only where accepting a malformed result
    /// would corrupt state or make cleanup ambiguous. Cosmetic fields are
    /// normalized instead: empty messages/details are removed and command paths
    /// are ignored because command changes are described by `detail`, not by a
    /// target-host filesystem path.
    ///
    /// Changed `fs_entry` records are state resources, so they must identify a
    /// non-empty absolute target-host path under the backend path semantics.
    pub fn normalize_apply_contract(&mut self, paths: &impl super::PathSemantics) -> Result<(), String> {
        trim_empty_string(&mut self.message);

        for (idx, change) in self.changes.iter_mut().enumerate() {
            let field = format!("changes[{}]", idx + 1);
            trim_empty_string(&mut change.detail);

            match change.subject {
                ChangeSubject::FsEntry => validate_fs_entry_change(change, paths, &field)?,
                ChangeSubject::Command => {
                    // A command result has no path semantics. Older or generic
                    // module helpers may accidentally carry a `path`; dropping
                    // it is safer and less surprising than failing a task for
                    // a field that Wali does not consume for command changes.
                    change.path = None;
                }
            }
        }

        Ok(())
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

fn validate_fs_entry_change(
    change: &mut ExecutionChange,
    paths: &impl super::PathSemantics,
    field: &str,
) -> Result<(), String> {
    let context = fs_entry_context(change.kind);

    if change.path.is_none() {
        if change.kind.changed() {
            return Err(format!("{field}.path is required for {context}"));
        }
        return Ok(());
    }

    if change.path.as_ref().is_some_and(|path| path.as_str().trim().is_empty()) {
        change.path = None;
        if change.kind.changed() {
            return Err(format!("{field}.path must not be empty for {context}"));
        }
        return Ok(());
    }

    if change.kind.changed() && change.path.as_ref().is_some_and(|path| !paths.is_absolute(path)) {
        return Err(format!("{field}.path must be absolute for {context}"));
    }

    Ok(())
}

fn fs_entry_context(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Created => "created fs_entry change",
        ChangeKind::Updated => "updated fs_entry change",
        ChangeKind::Removed => "removed fs_entry change",
        ChangeKind::Unchanged => "unchanged fs_entry change",
    }
}

fn trim_empty_string(value: &mut Option<String>) {
    if value.as_deref().is_some_and(|value| value.trim().is_empty()) {
        *value = None;
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationResult {
    pub ok: bool,
    pub message: Option<String>,
}
