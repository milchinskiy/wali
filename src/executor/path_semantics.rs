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

pub(crate) fn is_absolute_posix(path: &TargetPath) -> bool {
    path.as_str().starts_with('/')
}

pub(crate) fn basename_posix(path: &TargetPath) -> Option<String> {
    let normalized = normalize_posix(path);
    let value = normalized.as_str();

    if value == "/" || value == "." {
        return None;
    }

    value
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

pub(crate) fn strip_prefix_posix(base: &TargetPath, path: &TargetPath) -> Option<TargetPath> {
    let base = normalize_posix(base);
    let path = normalize_posix(path);
    let base_value = base.as_str();
    let path_value = path.as_str();

    if is_absolute_posix(&base) != is_absolute_posix(&path) {
        return None;
    }

    if base_value == path_value {
        return Some(TargetPath::new("."));
    }

    if base_value == "/" {
        return path_value
            .strip_prefix('/')
            .filter(|suffix| !suffix.is_empty())
            .map(TargetPath::from);
    }

    if base_value == "." {
        if path_value.starts_with("../") || path_value == ".." {
            return None;
        }
        return Some(TargetPath::from(path_value));
    }

    let prefix = format!("{base_value}/");
    path_value.strip_prefix(&prefix).map(TargetPath::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> TargetPath {
        TargetPath::from(value)
    }

    #[test]
    fn strip_prefix_is_normalized_and_segment_aware() {
        assert_eq!(strip_prefix_posix(&path("/tmp/app"), &path("/tmp/app/file")), Some(path("file")));
        assert_eq!(strip_prefix_posix(&path("/tmp/app"), &path("/tmp/app")), Some(path(".")));
        assert_eq!(strip_prefix_posix(&path("/tmp/app"), &path("/tmp/app2/file")), None);
        assert_eq!(strip_prefix_posix(&path("/tmp/app"), &path("/tmp/app/dir/../file")), Some(path("file")));
        assert_eq!(strip_prefix_posix(&path("app"), &path("/tmp/app/file")), None);
    }

    #[test]
    fn basename_uses_normalized_final_segment() {
        assert_eq!(basename_posix(&path("/tmp/app/dir/../file.txt")), Some("file.txt".to_owned()));
        assert_eq!(basename_posix(&path("/")), None);
        assert_eq!(basename_posix(&path(".")), None);
    }
}
