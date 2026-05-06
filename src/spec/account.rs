#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Group {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum User {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub user: Option<User>,
    pub group: Option<Group>,
}

impl Owner {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(User::Name(name)) = &self.user {
            validate_account_name("owner.user", name)?;
        }
        if let Some(Group::Name(name)) = &self.group {
            validate_account_name("owner.group", name)?;
        }
        Ok(())
    }
}

fn validate_account_name(field: &str, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if name.contains(':') {
        return Err(format!("{field} must not contain ':'"));
    }
    if name.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}
