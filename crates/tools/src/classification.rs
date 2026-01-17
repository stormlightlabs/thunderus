use std::collections::HashSet;

use thunderus_core::{Classification, ToolRisk};

/// Commands that run tests
///
/// FIXME: pytest command forms
const SAFE_TEST_COMMANDS: &[&str] = &[
    "test",
    "pytest",
    "pytest",
    "go test",
    "npm test",
    "yarn test",
    "make test",
];

/// Commands that format or lint code
const SAFE_FORMATTER_COMMANDS: &[&str] = &[
    "fmt", "format", "lint", "clippy", "eslint", "prettier", "black", "ruff", "stylua",
];

/// Read-only file system commands
const SAFE_READONLY_COMMANDS: &[&str] = &[
    "cat", "head", "tail", "grep", "find", "ls", "pwd", "echo", "print", "type", "which", "where", "whereis",
];

/// Text processing tools (read-only)
const SAFE_TEXT_READONLY_COMMANDS: &[&str] = &["grep", "egrep", "fgrep", "rg"];

/// Text processing tools (potentially risky with specific flags)
const TEXT_TOOLS_WITH_FLAGS: &[(&str, &[(&str, ToolRisk)])] = &[
    ("sed", &[("-i", ToolRisk::Risky), ("--in-place", ToolRisk::Risky)]),
    ("awk", &[(">", ToolRisk::Risky), (">>", ToolRisk::Risky)]),
];

/// Git read operations
const SAFE_GIT_READ_COMMANDS: &[&str] = &["git log", "git show", "git diff", "git status"];

/// Check and verify commands
const SAFE_VERIFY_COMMANDS: &[&str] = &["check", "verify", "validate"];

/// Patterns for file deletion operations
const RISKY_DELETION_PATTERNS: &[(&str, Pattern)] = &[
    ("rm", Pattern::Prefix("rm")),
    ("rmdir", Pattern::Exact("rmdir")),
    ("del", Pattern::Prefix("del")),
    ("shred", Pattern::Prefix("shred")),
];

/// Patterns for package installation/management
const RISKY_PACKAGE_PATTERNS: &[(&str, Pattern)] = &[
    ("install", Pattern::Contains("install")),
    ("uninstall", Pattern::Contains("uninstall")),
    ("apt-get", Pattern::Prefix("apt-get")),
    ("apt", Pattern::Prefix("apt")),
    ("yum", Pattern::Prefix("yum")),
    ("dnf", Pattern::Prefix("dnf")),
    ("brew", Pattern::Prefix("brew")),
    ("npm install", Pattern::Contains("install")),
    ("yarn add", Pattern::Contains("add")),
    ("yarn remove", Pattern::Contains("remove")),
    ("pip install", Pattern::Contains("install")),
    ("pip3 install", Pattern::Contains("install")),
    ("cargo install", Pattern::Contains("install")),
    ("go get", Pattern::Contains("get")),
    ("composer require", Pattern::Contains("require")),
];

/// Patterns for file system modifications
const RISKY_FILEMODIFY_PATTERNS: &[(&str, Pattern)] = &[
    ("mv", Pattern::Prefix("mv")),
    ("cp", Pattern::Prefix("cp")),
    ("chmod", Pattern::Prefix("chmod")),
    ("chown", Pattern::Prefix("chown")),
    ("touch", Pattern::Prefix("touch")),
    ("mkdir", Pattern::Prefix("mkdir")),
];

/// Patterns for network operations
const RISKY_NETWORK_PATTERNS: &[(&str, Pattern)] = &[
    ("curl", Pattern::Prefix("curl")),
    ("wget", Pattern::Prefix("wget")),
    ("nc", Pattern::Prefix("nc")),
    ("telnet", Pattern::Prefix("telnet")),
    ("ssh", Pattern::Prefix("ssh")),
    ("rsync", Pattern::Prefix("rsync")),
    ("scp", Pattern::Prefix("scp")),
];

/// Patterns for shell access and piping
const RISKY_SHELL_PATTERNS: &[(&str, Pattern)] = &[
    ("shell", Pattern::Exact("shell")),
    ("bash", Pattern::Prefix("bash")),
    ("zsh", Pattern::Prefix("zsh")),
    ("sh", Pattern::Prefix("sh")),
    ("fish", Pattern::Prefix("fish")),
];

/// Patterns for git write operations
const RISKY_GIT_WRITE_PATTERNS: &[(&str, Pattern)] = &[
    ("push", Pattern::Contains("push")),
    ("commit", Pattern::Contains("commit")),
    ("rebase", Pattern::Contains("rebase")),
];

/// Pattern matching type for command classification
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Exact match (e.g., "rmdir" only matches "rmdir")
    Exact(&'static str),
    /// Prefix match (e.g., "rm" matches "rm -rf", "rm file")
    Prefix(&'static str),
    /// Contains match (e.g., "install" matches "npm install", "cargo install")
    Contains(&'static str),
}

/// Classifies shell commands and tool operations as safe or risky
pub struct CommandClassifier {
    /// All commands considered safe
    safe_commands: HashSet<&'static str>,

    /// All patterns considered risky
    risky_patterns: Vec<(&'static str, Pattern)>,
}

impl CommandClassifier {
    /// Creates a new classifier with default safe/risky patterns
    pub fn new() -> Self {
        let safe_commands: HashSet<&'static str> = SAFE_TEST_COMMANDS
            .iter()
            .chain(SAFE_FORMATTER_COMMANDS.iter())
            .chain(SAFE_READONLY_COMMANDS.iter())
            .chain(SAFE_GIT_READ_COMMANDS.iter())
            .chain(SAFE_VERIFY_COMMANDS.iter())
            .copied()
            .collect();

        let risky_patterns: Vec<(&'static str, Pattern)> = RISKY_DELETION_PATTERNS
            .iter()
            .chain(RISKY_PACKAGE_PATTERNS.iter())
            .chain(RISKY_FILEMODIFY_PATTERNS.iter())
            .chain(RISKY_NETWORK_PATTERNS.iter())
            .chain(RISKY_SHELL_PATTERNS.iter())
            .chain(RISKY_GIT_WRITE_PATTERNS.iter())
            .cloned()
            .collect();

        Self { safe_commands, risky_patterns }
    }

    /// Classifies a shell command string with reasoning
    ///
    /// Returns a [Classification] containing both the risk level and explanation.
    /// This enables teaching users the safety model through consistency.
    pub fn classify_with_reasoning(&self, command: &str) -> Classification {
        let command_lower = command.to_lowercase();

        if command_lower.contains('|') {
            let pipeline_commands: Vec<&str> = command_lower.split('|').collect();
            for cmd in pipeline_commands.iter() {
                let first_word = cmd.split_whitespace().next().unwrap_or("");
                if (first_word == "sed" || first_word == "awk")
                    && let Some((classification, suggestion)) = self.classify_text_tool_with_flags(cmd.trim())
                    && classification.risk.is_risky()
                {
                    let reasoning = format!(
                        "Pipeline contains risky command '{}': {}",
                        first_word, classification.reasoning
                    );
                    return Classification::new(ToolRisk::Risky, reasoning)
                        .with_suggestion(suggestion.unwrap_or_default());
                }
            }

            let first_word = command_lower.split_whitespace().next().unwrap_or("");
            if let Some(reasoning) = self.check_safe_reasoning(first_word, &command_lower) {
                return Classification::new(ToolRisk::Safe, reasoning);
            }
        }

        let first_word = command_lower.split_whitespace().next().unwrap_or("");

        if (first_word == "sed" || first_word == "awk")
            && let Some((classification, suggestion)) = self.classify_text_tool_with_flags(&command_lower)
        {
            if let Some(suggestion_text) = suggestion {
                return classification.with_suggestion(suggestion_text);
            }
            return classification;
        }

        if let Some(reasoning) = self.check_safe_reasoning(first_word, &command_lower) {
            return Classification::new(ToolRisk::Safe, reasoning);
        }

        if let Some(reasoning) = self.check_risky_reasoning(first_word, &command_lower) {
            return Classification::new(ToolRisk::Risky, reasoning);
        }

        Classification::new(
            ToolRisk::Safe,
            format!(
                "Command '{}' is not in the known safe or risky lists, defaulting to safe",
                first_word
            ),
        )
    }

    /// Classifies a shell command string
    ///
    /// Returns [ToolRisk::Safe] for known safe commands,
    /// [ToolRisk::Risky] for known risky patterns,
    /// [ToolRisk::Safe] by default (conservative)
    ///
    /// For reasoning, use [classify_with_reasoning].
    pub fn classify_command(&self, command: &str) -> ToolRisk {
        self.classify_with_reasoning(command).risk
    }

    /// Checks if command is safe and returns reasoning
    fn check_safe_reasoning(&self, first_word: &str, command_lower: &str) -> Option<String> {
        if SAFE_TEST_COMMANDS.iter().any(|cmd| command_lower.contains(cmd)) {
            return Some("Test commands are read-only and have no side effects on files or system state".to_string());
        }

        if SAFE_FORMATTER_COMMANDS.iter().any(|cmd| command_lower.contains(cmd)) {
            return Some("Formatters and linters only modify code style, not behavior or functionality".to_string());
        }
        if SAFE_TEXT_READONLY_COMMANDS.contains(&first_word) {
            return Some(format!(
                "Command '{}' is a text search tool that only reads and matches patterns; it does not modify files",
                first_word
            ));
        }

        if SAFE_READONLY_COMMANDS.contains(&first_word) {
            return Some(format!(
                "Command '{}' only reads files or displays information; it does not modify anything",
                first_word
            ));
        }

        if SAFE_GIT_READ_COMMANDS.iter().any(|cmd| command_lower.contains(cmd)) {
            return Some(
                "Git read-only operations (log, diff, show, status) do not modify repository state".to_string(),
            );
        }

        if SAFE_VERIFY_COMMANDS.contains(&first_word) {
            return Some(format!(
                "Command '{}' only checks or validates; it does not make any changes",
                first_word
            ));
        }

        if (first_word == "sed" || first_word == "awk")
            && let Some((classification, _suggestion)) = self.classify_text_tool_with_flags(command_lower)
            && classification.risk.is_safe()
        {
            return Some(classification.reasoning);
        }

        None
    }

    /// Checks if command is risky and returns reasoning
    fn check_risky_reasoning(&self, first_word: &str, command_lower: &str) -> Option<String> {
        if (first_word == "sed" || first_word == "awk")
            && let Some((classification, _suggestion)) = self.classify_text_tool_with_flags(command_lower)
            && classification.risk.is_risky()
        {
            return Some(classification.reasoning);
        }

        for (desc, pattern) in &self.risky_patterns {
            match pattern {
                Pattern::Exact(cmd) if first_word == *cmd => {
                    return Some(match desc {
                        &"rm" | &"rmdir" | &"del" | &"shred" => {
                            format!(
                                "Command '{}' permanently deletes files or directories (destructive operation)",
                                first_word
                            )
                        }
                        _ => format!("Command '{}' is classified as risky because: {}", first_word, desc),
                    });
                }
                Pattern::Prefix(prefix) if first_word.starts_with(*prefix) => {
                    return Some(match desc {
                        &"rm" | &"rmdir" | &"del" | &"shred" => {
                            format!(
                                "Command '{}' permanently deletes files or directories (destructive operation)",
                                first_word
                            )
                        }
                        &"curl" | &"wget" | &"nc" | &"telnet" | &"ssh" | &"rsync" | &"scp" => {
                            format!(
                                "Command '{}' performs network operations which may transfer data to/from external systems",
                                first_word
                            )
                        }
                        &"mv" | &"cp" | &"chmod" | &"chown" | &"touch" | &"mkdir" => {
                            format!(
                                "Command '{}' modifies the file system structure or permissions",
                                first_word
                            )
                        }
                        &"apt-get" | &"apt" | &"yum" | &"dnf" | &"brew" => {
                            format!(
                                "Command '{}' is a package manager that may install software or modify system state",
                                first_word
                            )
                        }
                        &"bash" | &"zsh" | &"sh" | &"fish" | &"shell" => {
                            format!(
                                "Command '{}' opens an interactive shell which could execute arbitrary commands",
                                first_word
                            )
                        }
                        _ => format!("Command '{}' is classified as risky because: {}", first_word, desc),
                    });
                }
                Pattern::Contains(substr) if command_lower.contains(*substr) => {
                    return Some(match desc {
                        &"install" | &"uninstall" => {
                            format!(
                                "Command '{}' installs or removes packages which may modify dependencies or system state",
                                first_word
                            )
                        }
                        &"add" | &"remove" | &"require" | &"get" => {
                            format!(
                                "Command '{}' modifies dependencies (adds or removes packages)",
                                first_word
                            )
                        }
                        &"push" | &"commit" | &"rebase" => {
                            format!(
                                "Git command '{}' modifies repository history or pushes changes to remote",
                                substr
                            )
                        }
                        _ => format!("Command '{}' is classified as risky because it: {}", first_word, desc),
                    });
                }
                _ => {}
            }
        }

        None
    }

    /// Adds a custom safe command to the classifier
    pub fn add_safe_command(&mut self, command: &'static str) {
        self.safe_commands.insert(command);
    }

    /// Adds a custom risky pattern to the classifier
    pub fn add_risky_pattern(&mut self, desc: &'static str, pattern: Pattern) {
        self.risky_patterns.push((desc, pattern));
    }

    /// Returns set of safe commands
    pub fn safe_commands(&self) -> &HashSet<&'static str> {
        &self.safe_commands
    }

    /// Returns risky patterns
    pub fn risky_patterns(&self) -> &[(&'static str, Pattern)] {
        &self.risky_patterns
    }

    /// Classifies text processing tools (sed, awk) with nuanced risk detection based on flags
    ///
    /// Returns Some((classification, suggestion)) if the command is a text tool, None otherwise
    fn classify_text_tool_with_flags(&self, command_lower: &str) -> Option<(Classification, Option<String>)> {
        let parts: Vec<&str> = command_lower.split_whitespace().collect();
        let first_word = parts.first()?;

        let tool_config = TEXT_TOOLS_WITH_FLAGS.iter().find(|(tool, _)| *tool == *first_word)?;

        let risky_flags: Vec<&str> = tool_config
            .1
            .iter()
            .filter_map(|(flag, risk)| {
                if risk.is_risky() {
                    parts.iter().find(|p| **p == *flag || p.starts_with(*flag)).copied()
                } else {
                    None
                }
            })
            .collect();

        match *first_word {
            "sed" => {
                let has_in_place = !risky_flags.is_empty();

                if has_in_place {
                    Some((
                        Classification::new(
                            ToolRisk::Risky,
                            format!(
                                "Command 'sed' with {} flag modifies files in-place (destructive operation)",
                                risky_flags.join("/")
                            ),
                        ),
                        Some(
                            "Use the Edit tool for safer find-replace operations with validation and rollback"
                                .to_string(),
                        ),
                    ))
                } else {
                    Some((
                        Classification::new(
                            ToolRisk::Safe,
                            "Command 'sed' without -i flag only outputs transformed text to stdout; it does not modify files".to_string(),
                        ),
                        None,
                    ))
                }
            }
            "awk" => {
                let has_redirection = !risky_flags.is_empty();

                if has_redirection {
                    Some((
                        Classification::new(
                            ToolRisk::Risky,
                            format!(
                                "Command 'awk' with output redirection ({}) writes to files (destructive operation)",
                                risky_flags.join("/")
                            ),
                        ),
                        Some("Use the Read and Edit tools for safer file manipulation with validation".to_string()),
                    ))
                } else {
                    Some((
                        Classification::new(
                            ToolRisk::Safe,
                            "Command 'awk' without output redirection only outputs to stdout; it does not modify files"
                                .to_string(),
                        ),
                        None,
                    ))
                }
            }
            _ => None,
        }
    }
}

impl Default for CommandClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classifier_safe_test_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("cargo test"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("npm test"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("pytest"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("make test"), ToolRisk::Safe);
    }

    #[test]
    fn test_classifier_safe_linter_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("cargo clippy"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("eslint ."), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("prettier --write ."), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("black ."), ToolRisk::Safe);
    }

    #[test]
    fn test_classifier_safe_readonly_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("cat file.txt"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("grep pattern file"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("ls -la"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("git log"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("git diff HEAD"), ToolRisk::Safe);
    }

    #[test]
    fn test_classifier_risky_deletion_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("rm -rf /tmp"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("rmdir /tmp/dir"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("del file.txt"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("shred file"), ToolRisk::Risky);
    }

    #[test]
    fn test_classifier_risky_package_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("npm install lodash"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("pip install requests"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("cargo install ripgrep"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("apt-get install vim"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("brew install python"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("yarn add react"), ToolRisk::Risky);
    }

    #[test]
    fn test_classifier_risky_network_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(
            classifier.classify_command("curl https://api.example.com"),
            ToolRisk::Risky
        );
        assert_eq!(
            classifier.classify_command("wget https://example.com/file.txt"),
            ToolRisk::Risky
        );
        assert_eq!(classifier.classify_command("ssh user@host"), ToolRisk::Risky);
        assert_eq!(
            classifier.classify_command("scp file.txt user@host:/tmp"),
            ToolRisk::Risky
        );
    }

    #[test]
    fn test_classifier_risky_file_modify_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("mv old.txt new.txt"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("cp src dst"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("mkdir /tmp/dir"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("chmod +x script.sh"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("touch newfile.txt"), ToolRisk::Risky);
    }

    #[test]
    fn test_classifier_risky_git_write_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("git push origin"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("git commit -m 'fix'"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("git rebase main"), ToolRisk::Risky);
    }

    #[test]
    fn test_classifier_custom_safe_command() {
        let mut classifier = CommandClassifier::new();

        classifier.add_safe_command("my-safe-tool");
        assert_eq!(classifier.classify_command("my-safe-tool"), ToolRisk::Safe);
    }

    #[test]
    fn test_classifier_custom_risky_pattern() {
        let mut classifier = CommandClassifier::new();

        classifier.add_risky_pattern("custom risky", Pattern::Contains("dangerous"));
        assert_eq!(
            classifier.classify_command("my-tool dangerous operation"),
            ToolRisk::Risky
        );
    }

    #[test]
    fn test_classifier_case_insensitive() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("CARGO TEST"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("RM file"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("CURL http://example.com"), ToolRisk::Risky);
    }

    #[test]
    fn test_classifier_default_safe() {
        let classifier = CommandClassifier::new();
        assert_eq!(classifier.classify_command("unknown-command arg"), ToolRisk::Safe);
    }

    #[test]
    fn test_classify_with_reasoning_safe_test() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("cargo test");

        eprintln!("Reasoning for 'cargo test': {}", result.reasoning);
        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.to_lowercase().contains("test"));
        assert!(result.reasoning.contains("read-only"));
    }

    #[test]
    fn test_classify_with_reasoning_safe_formatter() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("cargo clippy");

        eprintln!("Reasoning for 'cargo clippy': {}", result.reasoning);
        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.to_lowercase().contains("formatter"));
        assert!(result.reasoning.to_lowercase().contains("linter"));
    }

    #[test]
    fn test_classify_with_reasoning_safe_readonly() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("cat file.txt");

        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.contains("reads"));
        assert!(result.reasoning.contains("does not modify"));
    }

    #[test]
    fn test_classify_with_reasoning_risky_deletion() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("rm -rf /tmp");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("deletes"));
        assert!(result.reasoning.contains("destructive"));
    }

    #[test]
    fn test_classify_with_reasoning_risky_network() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("curl https://api.example.com");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("network"));
        assert!(result.reasoning.contains("external"));
    }

    #[test]
    fn test_classify_with_reasoning_risky_package() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("npm install lodash");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("package"));
        assert!(result.reasoning.contains("modify"));
    }

    #[test]
    fn test_classify_with_reasoning_risky_file_modify() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("chmod +x script.sh");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("modifies"));
        assert!(result.reasoning.contains("file system"));
    }

    #[test]
    fn test_classify_with_reasoning_risky_git_write() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("git push origin main");

        eprintln!("Reasoning for 'git push origin main': {}", result.reasoning);
        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.to_lowercase().contains("git"));
        assert!(result.reasoning.contains("modifies"));
    }

    #[test]
    fn test_classify_with_reasoning_default_safe() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("unknown-command arg");

        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.contains("not in the known"));
        assert!(result.reasoning.contains("defaulting to safe"));
    }

    #[test]
    fn test_classifier_complex_commands() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("cargo test -- --nocapture"), ToolRisk::Safe);
        assert_eq!(
            classifier.classify_command("rm -rf /tmp/dir && rm file.txt"),
            ToolRisk::Risky
        );
        assert_eq!(
            classifier.classify_command("npm install --save-dev typescript"),
            ToolRisk::Risky
        );
        assert_eq!(classifier.classify_command("git log --oneline -10"), ToolRisk::Safe);
    }

    #[test]
    fn test_safe_commands_accessor() {
        let classifier = CommandClassifier::new();

        let safe_commands = classifier.safe_commands();
        assert!(!safe_commands.is_empty());
        assert!(safe_commands.contains("test"));
        assert!(safe_commands.contains("fmt"));
    }

    #[test]
    fn test_risky_patterns_accessor() {
        let classifier = CommandClassifier::new();

        let patterns = classifier.risky_patterns();
        assert!(!patterns.is_empty());
        assert!(patterns.iter().any(|(desc, _)| *desc == "rm"));
        assert!(patterns.iter().any(|(desc, _)| *desc == "install"));
    }

    #[test]
    fn test_text_tool_grep_safe() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("grep pattern file.txt");

        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.contains("text search"));
        assert!(result.reasoning.contains("does not modify"));
        assert!(result.suggestion.is_none());
    }

    #[test]
    fn test_text_tool_rg_safe() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("rg pattern src/");

        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.contains("text search"));
        assert!(result.suggestion.is_none());
    }

    #[test]
    fn test_text_tool_sed_safe() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("sed 's/old/new/g' file.txt");

        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.contains("stdout"));
        assert!(result.reasoning.contains("does not modify"));
        assert!(result.suggestion.is_none());
    }

    #[test]
    fn test_text_tool_sed_risky_with_flag() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("sed -i 's/old/new/g' file.txt");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("-i"));
        assert!(result.reasoning.contains("in-place"));
        assert!(result.suggestion.is_some());
        assert!(result.suggestion.as_ref().unwrap().contains("Edit tool"));
    }

    #[test]
    fn test_text_tool_sed_risky_with_long_flag() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("sed --in-place 's/old/new/g' file.txt");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("--in-place"));
        assert!(result.suggestion.is_some());
    }

    #[test]
    fn test_text_tool_awk_safe() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("awk '{print $1}' file.txt");

        assert_eq!(result.risk, ToolRisk::Safe);
        assert!(result.reasoning.contains("stdout"));
        assert!(result.reasoning.contains("does not modify"));
        assert!(result.suggestion.is_none());
    }

    #[test]
    fn test_text_tool_awk_risky_with_redirection() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("awk '{print $1}' file.txt > output.txt");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("redirection"));
        assert!(result.reasoning.contains("writes to files"));
        assert!(result.suggestion.is_some());
        assert!(result.suggestion.as_ref().unwrap().contains("Read and Edit"));
    }

    #[test]
    fn test_text_tool_awk_risky_with_append() {
        let classifier = CommandClassifier::new();
        let result = classifier.classify_with_reasoning("awk '{print $1}' file.txt >> output.txt");

        assert_eq!(result.risk, ToolRisk::Risky);
        assert!(result.reasoning.contains("redirection"));
        assert!(result.suggestion.is_some());
    }

    #[test]
    fn test_text_tool_case_insensitive() {
        let classifier = CommandClassifier::new();

        assert_eq!(classifier.classify_command("GREP pattern file"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("RG pattern src"), ToolRisk::Safe);
        assert_eq!(classifier.classify_command("SED -i 's/a/b/' file"), ToolRisk::Risky);
        assert_eq!(classifier.classify_command("AWK '{print}' file > out"), ToolRisk::Risky);
    }

    #[test]
    fn test_text_tool_complex_pipelines() {
        let classifier = CommandClassifier::new();

        assert_eq!(
            classifier.classify_command("cat file.txt | grep pattern | sed 's/foo/bar/'"),
            ToolRisk::Safe
        );

        assert_eq!(
            classifier.classify_command("grep pattern file.txt | sed -i 's/foo/bar/' output.txt"),
            ToolRisk::Risky
        );
    }

    #[test]
    fn test_text_tool_suggestions_are_pedagogical() {
        let classifier = CommandClassifier::new();

        let sed_risky = classifier.classify_with_reasoning("sed -i 's/old/new/g' file.txt");
        assert!(sed_risky.suggestion.as_ref().unwrap().contains("safer"));
        assert!(sed_risky.suggestion.as_ref().unwrap().contains("validation"));

        let awk_risky = classifier.classify_with_reasoning("awk '{print $1}' file.txt > out.txt");
        assert!(awk_risky.suggestion.as_ref().unwrap().contains("safer"));
        assert!(awk_risky.suggestion.as_ref().unwrap().contains("validation"));
    }
}
