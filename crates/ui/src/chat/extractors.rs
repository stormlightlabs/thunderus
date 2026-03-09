use serde_json::Value;

pub fn diff_path(diff: &str) -> Option<String> {
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            return Some(path.to_string());
        }
    }
    None
}

pub fn bash_cmd(arguments: &str) -> Option<String> {
    string_arg(arguments, "command")
}

pub fn edit_diff(arguments: &str) -> Option<String> {
    string_arg(arguments, "diff")
}

pub fn path_arg(arguments: &str) -> Option<String> {
    string_arg(arguments, "path")
}

pub fn research_url(arguments: &str) -> Option<String> {
    string_arg(arguments, "url")
}

pub fn string_arg(arguments: &str, key: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(arguments).ok()?;
    let object = value.as_object()?;
    object.get(key).and_then(Value::as_str).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_edit_diff_from_arguments() {
        let args = r#"{"path":"src/lib.rs","diff":"@@ -1,1 +1,1 @@\n-old\n+new"}"#;
        let diff = edit_diff(args).expect("diff should be present");
        assert!(diff.contains("@@ -1,1 +1,1 @@"));
    }

    #[test]
    fn test_extract_diff_path_from_unified_diff() {
        let diff = "--- a/src/old.rs\n+++ b/src/new.rs\n@@ -1 +1 @@\n-old\n+new";
        assert_eq!(diff_path(diff).as_deref(), Some("src/new.rs"));
    }
}
