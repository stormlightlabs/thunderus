//! Hunk labeling with semantic intent extraction
//!
//! This module analyzes hunks from unified diffs and assigns semantic labels
//! that describe the intent of the changes (e.g., "Add error handling", "Remove deprecated fn").
//!
//! The labeling system uses pattern matching to detect common code change patterns
//! and provides clear, human-readable labels that teach version control discipline.

use std::collections::HashSet;

use thunderus_core::patch::Hunk;

/// A hunk label describing the semantic intent of changes
#[derive(Debug, Clone, PartialEq)]
pub struct HunkLabel {
    /// The primary intent label (e.g., "Add error handling")
    pub intent: String,
    /// Secondary tags for additional context (e.g., ["function", "public"])
    pub tags: Vec<String>,
    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,
}

impl HunkLabel {
    /// Create a new hunk label
    pub fn new(intent: String) -> Self {
        Self { intent, tags: Vec::new(), confidence: 0.5 }
    }

    /// Add a tag to the label
    pub fn with_tag(mut self, tag: String) -> Self {
        self.tags.push(tag);
        self
    }

    /// Set the confidence score
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Format the label as a string for display
    pub fn display(&self) -> String {
        if self.tags.is_empty() {
            self.intent.clone()
        } else {
            format!("{} ({})", self.intent, self.tags.join(", "))
        }
    }
}

/// Pattern matchers for detecting common code change patterns
struct PatternMatchers {
    /// Keywords that indicate adding error handling
    error_handling: HashSet<&'static str>,
    /// Keywords that indicate adding tests
    test_addition: HashSet<&'static str>,
    /// Keywords that indicate removing code
    removal: HashSet<&'static str>,
    /// Keywords that indicate refactoring
    refactoring: HashSet<&'static str>,
    /// Keywords that indicate adding dependencies
    dependencies: HashSet<&'static str>,
    /// Keywords that indicate documentation
    documentation: HashSet<&'static str>,
    /// Keywords that indicate adding type signatures
    types: HashSet<&'static str>,
    /// Keywords that indicate performance improvements
    performance: HashSet<&'static str>,
    /// Keywords that indicate security changes
    security: HashSet<&'static str>,
}

impl Default for PatternMatchers {
    fn default() -> Self {
        Self {
            error_handling: [
                "error",
                "err",
                "result",
                "unwrap_or",
                "unwrap_or_else",
                "context",
                "anyhow",
                "bail",
                "ensure",
                "catch",
                "except",
                "throw",
                "raise",
                "try",
                "recover",
                "fallback",
                "handle",
                "validation",
                "validate",
            ]
            .iter()
            .cloned()
            .collect(),

            test_addition: [
                "test", "spec", "mock", "fixture", "assert", "expect", "should", "describe", "it(", "testcase",
                "pytest", "unittest",
            ]
            .iter()
            .cloned()
            .collect(),

            removal: [
                "deprecated",
                "obsolete",
                "remove",
                "delete",
                "unused",
                "legacy",
                "cleanup",
                "dead",
                "code",
            ]
            .iter()
            .cloned()
            .collect(),

            refactoring: [
                "extract",
                "inline",
                "rename",
                "reformat",
                "restructure",
                "simplify",
                "clarify",
                "reorganize",
                "consolidate",
                "split",
            ]
            .iter()
            .cloned()
            .collect(),

            dependencies: [
                "use ",
                "using ",
                "import ",
                "from ",
                "require(",
                "include",
                "dependency",
                "package",
                "module",
            ]
            .iter()
            .cloned()
            .collect(),

            documentation: [
                "///",
                "//",
                "/*",
                "*",
                "# ",
                "doc",
                "comment",
                "describe",
                "explanation",
                "note",
                "todo:",
                "fixme:",
            ]
            .iter()
            .cloned()
            .collect(),

            types: [
                "type ",
                "typedef",
                "interface",
                "struct",
                "class ",
                "enum",
                "annotation",
                "generic",
                "param",
                "return ",
            ]
            .iter()
            .cloned()
            .collect(),

            performance: [
                "cache",
                "lazy",
                "async",
                "await",
                "parallel",
                "concurrent",
                "optimize",
                "efficient",
                "fast",
                "slow",
                "performance",
                "profile",
                "benchmark",
            ]
            .iter()
            .cloned()
            .collect(),

            security: [
                "sanitize",
                "escape",
                "hash",
                "encrypt",
                "decrypt",
                "auth",
                "permission",
                "validate",
                "verify",
                "secure",
                "credential",
                "token",
                "csrf",
                "xss",
                "injection",
            ]
            .iter()
            .cloned()
            .collect(),
        }
    }
}

/// Hunk labeler for extracting semantic intent from hunks
pub struct HunkLabeler {
    patterns: PatternMatchers,
}

impl Default for HunkLabeler {
    fn default() -> Self {
        Self::new()
    }
}

impl HunkLabeler {
    /// Create a new hunk labeler with default patterns
    pub fn new() -> Self {
        Self { patterns: PatternMatchers::default() }
    }

    /// Analyze a hunk and extract its semantic intent
    ///
    /// This examines the hunk content to detect patterns and assign
    /// a meaningful label describing what the change accomplishes.
    pub fn label_hunk(&self, hunk: &Hunk) -> Option<HunkLabel> {
        let content = &hunk.content;
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return None;
        }

        let (additions, removals) = self.classify_changes(&lines);
        let total_changes = additions + removals;

        if total_changes == 0 {
            return None;
        }

        let keywords = self.extract_keywords(&lines);
        let label = self.match_intent(&keywords, additions, removals, total_changes);
        Some(label)
    }

    /// Classify lines as additions or removals
    fn classify_changes(&self, lines: &[&str]) -> (usize, usize) {
        let mut additions = 0;
        let mut removals = 0;

        for line in lines {
            if line.starts_with('+') && !line.starts_with("++") {
                additions += 1;
            } else if line.starts_with('-') && !line.starts_with("--") {
                removals += 1;
            }
        }

        (additions, removals)
    }

    /// Extract relevant keywords from hunk lines
    fn extract_keywords(&self, lines: &[&str]) -> HashSet<String> {
        let mut keywords = HashSet::new();

        for line in lines {
            if line.starts_with(' ') {
                continue;
            }

            let cleaned = line.trim_start_matches(['+', '-', ' ']).trim().to_lowercase();

            for word in cleaned.split_whitespace() {
                let word = word.trim_matches(['(', ')', '{', '}', '[', ']', ',', ';', ':', '.', '"', '\'']);
                if word.len() > 2 && !word.chars().all(|c| c.is_ascii_digit()) {
                    keywords.insert(word.to_string());
                }
            }
        }

        keywords
    }

    /// Match keywords against patterns to determine intent
    fn match_intent(&self, keywords: &HashSet<String>, additions: usize, removals: usize, _: usize) -> HunkLabel {
        let keyword_str: String = keywords.iter().map(|k| k.as_str()).collect::<Vec<&str>>().join(" ");

        let patterns = [
            (&self.patterns.security, "Security fix", 0.9),
            (&self.patterns.error_handling, "Add error handling", 0.8),
            (&self.patterns.performance, "Performance improvement", 0.75),
            (&self.patterns.test_addition, "Add tests", 0.8),
            (&self.patterns.types, "Add type annotations", 0.7),
            (&self.patterns.dependencies, "Add dependencies", 0.7),
            (&self.patterns.documentation, "Update documentation", 0.65),
            (&self.patterns.refactoring, "Refactor code", 0.6),
            (&self.patterns.removal, "Remove code", 0.7),
        ];

        for (pattern_set, intent, base_confidence) in patterns {
            if self.matches_any_pattern(&keyword_str, pattern_set) {
                let mut label = HunkLabel::new(intent.to_string()).with_confidence(base_confidence);

                if additions > removals * 2 {
                    label = label.with_tag("addition".to_string());
                } else if removals > additions * 2 {
                    label = label.with_tag("removal".to_string());
                } else {
                    label = label.with_tag("modification".to_string());
                }

                return label;
            }
        }

        if additions > removals * 2 {
            HunkLabel::new("Add code".to_string()).with_confidence(0.4)
        } else if removals > additions * 2 {
            HunkLabel::new("Remove code".to_string()).with_confidence(0.4)
        } else {
            HunkLabel::new("Modify code".to_string()).with_confidence(0.3)
        }
    }

    /// Check if the keyword string matches any pattern in the set
    fn matches_any_pattern(&self, keyword_str: &str, pattern_set: &HashSet<&'static str>) -> bool {
        for pattern in pattern_set {
            if keyword_str.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Label multiple hunks at once
    pub fn label_hunks(&self, hunks: &[Hunk]) -> Vec<Option<HunkLabel>> {
        hunks.iter().map(|h| self.label_hunk(h)).collect()
    }

    /// Create a labeler function suitable for use with `Patch::label_hunks`
    ///
    /// This returns a function that can be passed directly to `Patch::label_hunks`
    /// to automatically label all hunks in a patch.
    pub fn as_labeler_fn(&self) -> impl Fn(&Hunk) -> Option<String> + '_ {
        let labeler = self;
        move |hunk: &Hunk| labeler.label_hunk(hunk).map(|label| label.display())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_hunk(content: &str) -> Hunk {
        Hunk {
            old_start: 1,
            old_lines: content.lines().count(),
            new_start: 1,
            new_lines: content.lines().count(),
            content: content.to_string(),
            intent: None,
            approved: false,
        }
    }

    #[test]
    fn test_hunk_label_display() {
        let label = HunkLabel::new("Add error handling".to_string())
            .with_tag("function".to_string())
            .with_confidence(0.8);

        assert_eq!(label.display(), "Add error handling (function)");
        assert_eq!(label.intent, "Add error handling");
        assert_eq!(label.confidence, 0.8);
    }

    #[test]
    fn test_label_error_handling() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("-let x = unwrap();\n+let x = unwrap().context(\"Failed to parse\");");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Add error handling");
    }

    #[test]
    fn test_label_test_addition() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("+#[test]\n+fn test_example() {\n+    assert_eq!(1, 1);\n+}");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Add tests");
    }

    #[test]
    fn test_label_removal() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("-#[deprecated]\n-fn old_function() {\n-}");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Remove code");
    }

    #[test]
    fn test_label_documentation() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk(
            "+/// This function processes data\n+///\n+/// # Arguments\n+/// * `data` - The data to process",
        );

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Update documentation");
    }

    #[test]
    fn test_label_type_annotations() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("+struct Data {\n+    value: String,\n+    count: usize,\n+}");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Add type annotations");
    }

    #[test]
    fn test_label_performance() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("-for item in &items {\n+for item in items.iter() {\n+    cache.insert(item);");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Performance improvement");
    }

    #[test]
    fn test_label_security() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("-exec(query)\n+exec(sanitize(query))");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Security fix");
    }

    #[test]
    fn test_label_fallback_addition() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("+fn new_function() {\n+    println!(\"hello\");\n+}");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_some());
        assert_eq!(label.unwrap().intent, "Add code");
    }

    #[test]
    fn test_label_empty_hunk() {
        let labeler = HunkLabeler::new();
        let hunk = create_test_hunk("");

        let label = labeler.label_hunk(&hunk);
        assert!(label.is_none());
    }

    #[test]
    fn test_label_multiple_hunks() {
        let labeler = HunkLabeler::new();
        let hunks = vec![
            create_test_hunk("+#[test]\n+fn test_x() {}"),
            create_test_hunk("-let x = unwrap();\n+let x = unwrap().context(\"error\");"),
            create_test_hunk(" context line\n-only line"),
        ];

        let labels = labeler.label_hunks(&hunks);
        assert_eq!(labels.len(), 3);
        assert!(labels[0].is_some());
        assert!(labels[1].is_some());
        assert!(labels[2].is_some());
    }
}
