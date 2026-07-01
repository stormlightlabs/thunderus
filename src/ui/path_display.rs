use std::path::{Component, Path, PathBuf};

use crate::utils;

struct DisplayPathParts {
    prefix: String,
    components: Vec<DisplayPathComponent>,
}

impl DisplayPathParts {
    fn maybe(path: &Path, home: Option<PathBuf>) -> Option<DisplayPathParts> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let (prefix, base, rel) = match home {
            Some(home) => {
                let home = home.canonicalize().unwrap_or(home);
                match path.strip_prefix(&home) {
                    Ok(rest) => (String::from("~"), home, rest),
                    Err(_) => {
                        let root = path.components().next()?;
                        (
                            root_prefix(root),
                            match root {
                                Component::RootDir => PathBuf::from("/"),
                                _ => PathBuf::new(),
                            },
                            path.as_path(),
                        )
                    }
                }
            }
            None => {
                let root = path.components().next()?;
                let base = match root {
                    Component::RootDir => PathBuf::from("/"),
                    _ => PathBuf::new(),
                };
                (root_prefix(root), base, path.as_path())
            }
        };

        let mut parent = base;
        let mut components = Vec::new();
        for component in rel.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            components.push(DisplayPathComponent { name: name.to_string_lossy().to_string(), parent: parent.clone() });
            parent.push(name);
        }

        Some(DisplayPathParts { prefix, components })
    }
}

struct DisplayPathComponent {
    name: String,
    parent: PathBuf,
}

pub fn footer_segment(path: &Path, line_width: usize, used_width: usize) -> String {
    let prefix = "cwd: ";
    let budget = line_width.saturating_sub(used_width + 1 + prefix.len());
    let home = std::env::var_os("HOME").map(PathBuf::from);
    format!("{prefix}{}", cwd_display_for_width_with_home(path, budget, home))
}

fn cwd_display_for_width_with_home(path: &Path, max_width: usize, home: Option<PathBuf>) -> String {
    if max_width == 0 {
        return String::new();
    }

    let full = home_relative_display(path, home.clone());
    if utils::text_width(&full) <= max_width {
        return full;
    }

    let Some(parts) = DisplayPathParts::maybe(path, home) else {
        return utils::truncate_ellipsis_start(&full, max_width);
    };
    if parts.components.is_empty() {
        return utils::truncate_ellipsis_start(&full, max_width);
    }

    let compact_components = parts
        .components
        .iter()
        .enumerate()
        .map(|(i, part)| {
            if i + 1 == parts.components.len() {
                part.name.clone()
            } else {
                shortest_unique_prefix(&part.name, &part.parent)
            }
        })
        .collect::<Vec<_>>();
    let compact = render_path_parts(&parts.prefix, &compact_components);
    if utils::text_width(&compact) <= max_width {
        return compact;
    }

    for start in 1..compact_components.len() {
        let mut candidate = vec![String::from("…")];
        candidate.extend(compact_components[start..].iter().cloned());
        let rendered = render_path_parts(&parts.prefix, &candidate);
        if utils::text_width(&rendered) <= max_width {
            return rendered;
        }
    }

    utils::truncate_ellipsis_start(
        compact_components.last().map(String::as_str).unwrap_or(&full),
        max_width,
    )
}

fn home_relative_display(path: &Path, home: Option<PathBuf>) -> String {
    match home {
        Some(home) => {
            let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let home = home.canonicalize().unwrap_or(home);
            match path.strip_prefix(&home) {
                Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
                Ok(rest) => format!("~/{}", rest.display()),
                Err(_) => path.display().to_string(),
            }
        }
        None => path.display().to_string(),
    }
}

fn root_prefix(component: Component<'_>) -> String {
    match component {
        Component::RootDir => String::from("/"),
        Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().to_string(),
        _ => String::new(),
    }
}

fn render_path_parts(prefix: &str, components: &[String]) -> String {
    if components.is_empty() {
        prefix.to_string()
    } else {
        match prefix {
            "" => components.join("/"),
            "/" => format!("/{}", components.join("/")),
            _ => format!("{prefix}/{}", components.join("/")),
        }
    }
}

fn shortest_unique_prefix(name: &str, parent: &Path) -> String {
    let chars = name.chars().collect::<Vec<_>>();
    if chars.len() <= 1 {
        return name.to_string();
    }

    let siblings = match std::fs::read_dir(parent) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>(),
        Err(_) => return chars[0].to_string(),
    };

    for len in 1..chars.len() {
        let prefix = chars.iter().take(len).collect::<String>();
        let ambiguous = siblings
            .iter()
            .any(|sibling| sibling != name && sibling.starts_with(&prefix));
        if !ambiguous {
            return prefix;
        }
    }

    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn home_relative_display_shortens_paths_under_home() {
        let path = PathBuf::from("/home/owais/project");
        let home = PathBuf::from("/home/owais");
        assert_eq!(home_relative_display(&path, Some(home)), "~/project");
    }

    #[test]
    fn home_relative_display_shortens_home_itself() {
        let path = PathBuf::from("/home/owais");
        let home = PathBuf::from("/home/owais");
        assert_eq!(home_relative_display(&path, Some(home)), "~");
    }

    #[test]
    fn home_relative_display_leaves_paths_outside_home() {
        let path = PathBuf::from("/repo");
        let home = PathBuf::from("/home/owais");
        assert_eq!(home_relative_display(&path, Some(home)), "/repo");
    }

    #[test]
    fn home_relative_display_leaves_paths_when_home_is_missing() {
        let path = PathBuf::from("/repo");
        assert_eq!(home_relative_display(&path, None), "/repo");
    }

    #[test]
    fn cwd_display_for_width_keeps_full_path_when_it_fits() {
        let home = PathBuf::from("/home/owais");
        let path = home.join("Projects/thndrs");
        assert_eq!(
            cwd_display_for_width_with_home(&path, 40, Some(home)),
            "~/Projects/thndrs"
        );
    }

    #[test]
    fn cwd_display_for_width_abbreviates_ancestors_with_unique_prefixes() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let home = dir.path().join("home");
        let path = home.join("Projects/StormlightLabs/OpenSource/thndrs");
        fs::create_dir_all(&path).expect("mkdir path");
        fs::create_dir_all(home.join("Pictures")).expect("mkdir sibling");

        assert_eq!(
            cwd_display_for_width_with_home(&path, 20, Some(home)),
            "~/Pr/S/O/thndrs"
        );
    }

    #[test]
    fn cwd_display_for_width_keeps_longest_meaningful_suffix() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let home = dir.path().join("home");
        let path = home.join("Projects/StormlightLabs/OpenSource/thndrs");
        fs::create_dir_all(&path).expect("mkdir path");
        fs::create_dir_all(home.join("Pictures")).expect("mkdir sibling");
        assert_eq!(cwd_display_for_width_with_home(&path, 12, Some(home)), "~/…/O/thndrs");
    }

    #[test]
    fn cwd_display_for_width_preserves_current_dir_when_tiny() {
        let path = PathBuf::from("/alpha/beta/thndrs");
        assert_eq!(cwd_display_for_width_with_home(&path, 4, None), "…drs");
    }
}
