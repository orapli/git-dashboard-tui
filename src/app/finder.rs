use std::path::PathBuf;

pub fn sanitize_path_input(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if ((s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')))
        && s.len() >= 2
    {
        s = s[1..s.len() - 1].to_string();
    }
    s.replace("\\ ", " ")
        .replace("\\(", "(")
        .replace("\\)", ")")
}

pub fn complete_path(input: &str) -> Vec<String> {
    let cleaned = sanitize_path_input(input);
    let (prefix_display, search_path, filter_prefix) =
        if cleaned.is_empty() || cleaned == "." || cleaned == "./" {
            ("./".to_string(), PathBuf::from("."), String::new())
        } else if cleaned == ".." || cleaned == "../" {
            ("../".to_string(), PathBuf::from(".."), String::new())
        } else if cleaned == "~" {
            let home = home_dir().unwrap_or_else(|| PathBuf::from("/"));
            ("~/".to_string(), home, String::new())
        } else if let Some(stripped) = cleaned.strip_prefix("~/") {
            let home = home_dir().unwrap_or_else(|| PathBuf::from("/"));
            let expanded = home.join(stripped);
            if cleaned.ends_with('/') {
                (cleaned.clone(), expanded, String::new())
            } else {
                let parent_display = match cleaned.rfind('/') {
                    Some(idx) => cleaned[..=idx].to_string(),
                    None => "~/".to_string(),
                };
                let parent = expanded.parent().unwrap_or(&home).to_path_buf();
                let file_prefix = expanded
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                (parent_display, parent, file_prefix)
            }
        } else if cleaned.starts_with('/') {
            let p = PathBuf::from(&cleaned);
            if cleaned.ends_with('/') {
                (cleaned.clone(), p, String::new())
            } else {
                let parent_display = match cleaned.rfind('/') {
                    Some(idx) => cleaned[..=idx].to_string(),
                    None => "/".to_string(),
                };
                let parent = p
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("/"))
                    .to_path_buf();
                let file_prefix = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                (parent_display, parent, file_prefix)
            }
        } else {
            let p = PathBuf::from(&cleaned);
            if cleaned.ends_with('/') {
                (cleaned.clone(), p, String::new())
            } else if let Some(idx) = cleaned.rfind('/') {
                let parent_display = cleaned[..=idx].to_string();
                let parent = PathBuf::from(&cleaned[..idx]);
                let file_prefix = cleaned[idx + 1..].to_string();
                (parent_display, parent, file_prefix)
            } else {
                ("./".to_string(), PathBuf::from("."), cleaned.clone())
            }
        };

    let Ok(entries) = std::fs::read_dir(&search_path) else {
        return Vec::new();
    };

    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') && !filter_prefix.starts_with('.') {
            continue;
        }
        if name_str
            .to_lowercase()
            .starts_with(&filter_prefix.to_lowercase())
        {
            let completed = format!("{}{}/", prefix_display, name_str);
            matches.push(completed);
        }
    }
    matches.sort();
    matches
}

pub fn expand_user_path(raw: &str) -> PathBuf {
    let sanitized = sanitize_path_input(raw);
    let raw = sanitized.trim();
    if raw == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| PathBuf::from(raw));
    }
    PathBuf::from(raw)
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for x in chars.by_ref() {
                if x.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
