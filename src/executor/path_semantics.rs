use super::TargetPath;

pub(crate) fn join_posix(base: &TargetPath, child: &str) -> TargetPath {
    if child.starts_with('/') {
        return normalize_posix(&TargetPath::new(child));
    }

    let mut value = base.as_str().to_owned();
    if !value.is_empty() && !value.ends_with('/') {
        value.push('/');
    }
    value.push_str(child);
    normalize_posix(&TargetPath::new(value))
}

pub(crate) fn normalize_posix(path: &TargetPath) -> TargetPath {
    let raw = path.as_str();
    let absolute = raw.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();

    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => match parts.last().copied() {
                Some("..") | None if !absolute => parts.push(".."),
                Some(_) => {
                    parts.pop();
                }
                None => {}
            },
            _ => parts.push(part),
        }
    }

    let mut normalized = String::new();
    if absolute {
        normalized.push('/');
    }
    normalized.push_str(&parts.join("/"));

    if normalized.is_empty() {
        if absolute {
            normalized.push('/');
        } else {
            normalized.push('.');
        }
    }

    TargetPath::new(normalized)
}

pub(crate) fn parent_posix(path: &TargetPath) -> Option<TargetPath> {
    let normalized = normalize_posix(path);
    let value = normalized.as_str();

    if value == "/" || value == "." {
        return None;
    }

    if value.starts_with('/') {
        match value.rsplit_once('/') {
            Some(("", _)) => Some(TargetPath::new("/")),
            Some((parent, _)) => Some(TargetPath::new(parent)),
            None => None,
        }
    } else {
        value
            .rsplit_once('/')
            .map(|(parent, _)| TargetPath::new(if parent.is_empty() { "." } else { parent }))
    }
}
