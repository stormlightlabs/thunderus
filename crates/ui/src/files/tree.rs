use ignore::WalkBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(super) struct FileNode {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
    pub(super) children: Vec<FileNode>,
}

impl FileNode {
    pub(super) fn root(name: String) -> Self {
        Self { name, path: PathBuf::new(), is_dir: true, children: Vec::new() }
    }

    pub(super) fn sort_recursive(&mut self) {
        self.children.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()))
        });
        for child in &mut self.children {
            child.sort_recursive();
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TreeEntry {
    pub(super) name: String,
    pub(super) path: PathBuf,
    pub(super) depth: u16,
    pub(super) is_dir: bool,
    pub(super) expanded: bool,
}

pub(super) fn build_workspace_index(root: &Path) -> std::io::Result<(FileNode, Vec<PathBuf>)> {
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| root.display().to_string());
    let mut tree = FileNode::root(root_name);
    let mut files = Vec::new();

    let mut walker = WalkBuilder::new(root);
    walker
        .standard_filters(true)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .parents(true)
        .filter_entry(|entry| entry.file_name() != ".git");

    for dent in walker.build() {
        let Ok(entry) = dent else {
            continue;
        };

        let path = entry.path();
        if path == root {
            continue;
        }

        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };

        if relative.as_os_str().is_empty() {
            continue;
        }

        if is_git_metadata_path(relative) {
            continue;
        }

        let is_dir = entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false);
        insert_node(&mut tree, relative, is_dir);

        if !is_dir {
            files.push(relative.to_path_buf());
        }
    }

    files.sort();
    Ok((tree, files))
}

pub(super) fn flatten_tree_entries(
    node: &FileNode, depth: u16, expanded_dirs: &HashSet<PathBuf>, out: &mut Vec<TreeEntry>,
) {
    for child in &node.children {
        let expanded = child.is_dir && expanded_dirs.contains(&child.path);
        out.push(TreeEntry {
            name: child.name.clone(),
            path: child.path.clone(),
            depth,
            is_dir: child.is_dir,
            expanded,
        });

        if child.is_dir && expanded {
            flatten_tree_entries(child, depth.saturating_add(1), expanded_dirs, out);
        }
    }
}

pub(super) fn build_debug_tree() -> FileNode {
    let mut root = FileNode::root("debug-workspace".to_string());

    for dir in 0..14 {
        let mut dir_node = FileNode {
            name: format!("module_{dir:02}"),
            path: PathBuf::from(format!("module_{dir:02}")),
            is_dir: true,
            children: Vec::new(),
        };

        for nested in 0..6 {
            let nested_name = format!("feature_{nested:02}");
            let mut nested_node = FileNode {
                name: nested_name.clone(),
                path: PathBuf::from(format!("module_{dir:02}/{nested_name}")),
                is_dir: true,
                children: Vec::new(),
            };

            for file in 0..8 {
                let name = format!("file_{file:02}.rs");
                nested_node.children.push(FileNode {
                    name: name.clone(),
                    path: PathBuf::from(format!("module_{dir:02}/{nested_name}/{name}")),
                    is_dir: false,
                    children: Vec::new(),
                });
            }

            dir_node.children.push(nested_node);
        }

        root.children.push(dir_node);
    }

    root.sort_recursive();
    root
}

pub(super) fn collect_files(root: &FileNode) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_files_recursive(root, &mut files);
    files.sort();
    files
}

pub(super) fn workspace_files(root: &Path) -> Vec<PathBuf> {
    build_workspace_index(root).map(|(_, files)| files).unwrap_or_default()
}

fn is_git_metadata_path(relative_path: &Path) -> bool {
    relative_path
        .components()
        .next()
        .map(|component| component.as_os_str() == ".git")
        .unwrap_or(false)
}

fn insert_node(root: &mut FileNode, relative_path: &Path, is_dir: bool) {
    let components = relative_path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    if components.is_empty() {
        return;
    }

    let mut cursor = root;
    let mut current_path = PathBuf::new();

    for (idx, name) in components.iter().enumerate() {
        current_path.push(name);
        let last = idx + 1 == components.len();
        let node_is_dir = if last { is_dir } else { true };

        let child_index = if let Some(existing_idx) = cursor.children.iter().position(|child| child.name == *name) {
            existing_idx
        } else {
            cursor.children.push(FileNode {
                name: name.clone(),
                path: current_path.clone(),
                is_dir: node_is_dir,
                children: Vec::new(),
            });
            cursor.children.len() - 1
        };

        cursor = &mut cursor.children[child_index];
    }
}

fn collect_files_recursive(node: &FileNode, out: &mut Vec<PathBuf>) {
    for child in &node.children {
        if child.is_dir {
            collect_files_recursive(child, out);
        } else {
            out.push(child.path.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_insert_node_creates_nested_nodes() {
        let mut root = FileNode::root("root".to_string());
        insert_node(&mut root, Path::new("src/main.rs"), false);

        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "src");
        assert!(root.children[0].is_dir);
        assert_eq!(root.children[0].children[0].name, "main.rs");
        assert!(!root.children[0].children[0].is_dir);
    }

    #[test]
    fn test_collect_files_from_debug_tree() {
        let root = build_debug_tree();
        let files = collect_files(&root);
        assert!(!files.is_empty());
        assert!(
            files
                .iter()
                .all(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        );
    }

    #[test]
    fn test_workspace_files_excludes_git_metadata() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be valid")
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!("thndrs-ui-files-{unique}"));
        std::fs::create_dir_all(workspace.join(".git")).expect(".git directory should be created");
        std::fs::create_dir_all(workspace.join("src")).expect("src directory should be created");
        std::fs::write(workspace.join(".git/HEAD"), "ref: refs/heads/main\n").expect("git metadata should be written");
        std::fs::write(workspace.join("src/main.rs"), "fn main() {}\n").expect("workspace file should be written");

        let files = workspace_files(&workspace);

        assert!(files.iter().all(|path| !path.starts_with(".git")));
        assert!(files.iter().any(|path| path == Path::new("src/main.rs")));

        std::fs::remove_dir_all(workspace).expect("workspace should be removed");
    }
}
