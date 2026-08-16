//! Prompt-template discovery, invocation parsing, and MiniJinja rendering.
//!
//! Templates are application-owned resources loaded from built-ins,
//! `~/.thndrs/prompts`, and `<workspace>/.thndrs/prompts`. Discovery is
//! non-recursive. Later scopes replace earlier templates with the same name.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use minijinja::{Environment, UndefinedBehavior};
use serde::Deserialize;

use crate::trust::{self, ProjectTrust, ProjectTrustScope};
use crate::utils;

const MAX_TEMPLATE_BYTES: u64 = 256 * 1024;
const MAX_RENDERED_BYTES: usize = 256 * 1024;
const RENDER_FUEL: u64 = 50_000;

const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    ("adversarial-review.j2", include_str!("builtins/adversarial-review.j2")),
    ("changelog-audit.j2", include_str!("builtins/changelog-audit.j2")),
    ("commit.j2", include_str!("builtins/commit.j2")),
    ("issue.j2", include_str!("builtins/issue.j2")),
    ("pr-review.j2", include_str!("builtins/pr-review.j2")),
    ("review.j2", include_str!("builtins/review.j2")),
    ("security-advisory.j2", include_str!("builtins/security-advisory.j2")),
    ("wrap.j2", include_str!("builtins/wrap.j2")),
];

/// Origin of a loaded prompt template.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptTemplateSource {
    BuiltIn,
    User,
    Project,
}

impl PromptTemplateSource {
    /// Stable label used in diagnostics and UI details.
    pub fn label(self) -> &'static str {
        match self {
            Self::BuiltIn => "built-in",
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

/// One reusable slash-command prompt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptTemplate {
    /// Slash-command name derived from the filename.
    pub name: String,
    /// Compact text shown in command suggestions.
    pub description: String,
    /// Optional argument shape shown before the description.
    pub argument_hint: Option<String>,
    /// MiniJinja source without YAML frontmatter.
    pub body: String,
    /// Winning discovery scope for this command name.
    pub source: PromptTemplateSource,
    /// Filesystem source, or `None` for a bundled template.
    pub path: Option<PathBuf>,
}

/// A non-fatal template discovery problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptTemplateDiagnostic {
    /// Template file or synthetic bundled path.
    pub path: PathBuf,
    /// Human-readable discovery failure.
    pub message: String,
}

impl PromptTemplateDiagnostic {
    fn new(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self { path: path.into(), message: message.into() }
    }

    /// Compact startup diagnostic.
    pub fn summary(&self) -> String {
        format!("prompt template diagnostic  {}: {}", self.path.display(), self.message)
    }
}

/// Loaded templates plus non-fatal discovery diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PromptTemplateInventory {
    /// Valid templates after precedence has been applied.
    pub templates: Vec<PromptTemplate>,
    /// Non-fatal problems encountered during discovery.
    pub diagnostics: Vec<PromptTemplateDiagnostic>,
}

/// Parsed positional and named invocation arguments.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PromptTemplateArgs {
    /// Arguments without a valid `key=` prefix, in invocation order.
    pub positional: Vec<String>,
    /// Valid `key=value` arguments ordered by key.
    pub named: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Frontmatter {
    description: Option<String>,
    #[serde(rename = "argument-hint")]
    argument_hint: Option<String>,
}

#[derive(Debug, Default)]
struct BoundedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

impl io::Write for BoundedOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > MAX_RENDERED_BYTES {
            self.exceeded = true;
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "rendered prompt is too large",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Discover bundled, global, and project prompt templates.
pub fn discover(workspace_root: &Path) -> PromptTemplateInventory {
    let mut selected = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for (filename, raw) in BUILTIN_TEMPLATES {
        let path = PathBuf::from(format!("<built-in>/{filename}"));
        match parse_template(&path, raw, PromptTemplateSource::BuiltIn, None) {
            Ok(template) => {
                selected.insert(template.name.clone(), template);
            }
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    if let Some(home) = utils::home_dir() {
        load_directory(
            &home.join(".thndrs").join("prompts"),
            PromptTemplateSource::User,
            &mut selected,
            &mut diagnostics,
        );
    }
    let project_directory = workspace_root.join(".thndrs").join("prompts");
    match project_templates_active(workspace_root, &project_directory) {
        Ok(true) => load_directory(
            &project_directory,
            PromptTemplateSource::Project,
            &mut selected,
            &mut diagnostics,
        ),
        Ok(false) => diagnostics.push(PromptTemplateDiagnostic::new(
            project_directory,
            "project prompt templates are inactive because they have not been trusted; inspect with `thndrs trust status` and approve with `thndrs trust grant prompt-templates`",
        )),
        Err(message) => diagnostics.push(PromptTemplateDiagnostic::new(project_directory, message)),
    }

    PromptTemplateInventory { templates: selected.into_values().collect(), diagnostics }
}

fn project_templates_active(workspace_root: &Path, directory: &Path) -> Result<bool, String> {
    let roots = [directory.to_path_buf()];
    let Some(fingerprint) =
        trust::fingerprint_directories(workspace_root, &roots).map_err(|error| error.to_string())?
    else {
        return Ok(true);
    };
    match trust::project_trust(workspace_root, ProjectTrustScope::PromptTemplates, &fingerprint)
        .map_err(|error| error.to_string())?
    {
        ProjectTrust::Trusted => Ok(true),
        ProjectTrust::Untrusted | ProjectTrust::Stale { .. } => Ok(false),
    }
}

/// Parse shell-like positional arguments and `key=value` named arguments.
pub fn parse_args(input: &str) -> Result<PromptTemplateArgs, String> {
    let tokens = tokenize(input)?;
    let mut parsed = PromptTemplateArgs::default();

    for token in tokens {
        let Some((key, value)) = token.split_once('=') else {
            parsed.positional.push(token);
            continue;
        };
        if !valid_identifier(key) {
            parsed.positional.push(token);
            continue;
        }
        if reserved_name(key) {
            return Err(format!("named argument `{key}` is reserved by the template context"));
        }
        if parsed.named.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("named argument `{key}` was provided more than once"));
        }
    }

    Ok(parsed)
}

/// Render one prompt template with positional and named arguments.
pub fn render(template: &PromptTemplate, input: &str) -> Result<String, String> {
    let args = parse_args(input)?;
    let mut context = serde_json::Map::new();
    context.insert("args".to_string(), serde_json::json!(args.positional));
    context.insert("arguments".to_string(), serde_json::json!(args.positional.join(" ")));
    context.insert("named".to_string(), serde_json::json!(args.named));
    for (index, value) in args.positional.iter().enumerate() {
        context.insert(format!("arg{}", index + 1), serde_json::json!(value));
    }
    for (key, value) in &args.named {
        context.insert(key.clone(), serde_json::json!(value));
    }

    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    environment.set_fuel(Some(RENDER_FUEL));
    let compiled = environment
        .template_from_str(&template.body)
        .map_err(|error| format!("failed to render /{}: {error:#}", template.name))?;
    let mut output = BoundedOutput::default();
    let render_result = compiled.render_captured_to(serde_json::Value::Object(context), &mut output);
    if output.exceeded {
        return Err(format!(
            "failed to render /{}: output exceeds the {MAX_RENDERED_BYTES}-byte limit",
            template.name
        ));
    }
    render_result.map_err(|error| format!("failed to render /{}: {error:#}", template.name))?;
    let rendered = String::from_utf8(output.bytes)
        .map_err(|error| format!("failed to render /{} as UTF-8: {error}", template.name))?;
    if rendered.trim().is_empty() {
        return Err(format!("failed to render /{}: template output is empty", template.name));
    }
    Ok(rendered)
}

fn load_directory(
    directory: &Path, source: PromptTemplateSource, selected: &mut BTreeMap<String, PromptTemplate>,
    diagnostics: &mut Vec<PromptTemplateDiagnostic>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            diagnostics.push(PromptTemplateDiagnostic::new(
                directory,
                format!("failed to read prompt directory: {error}"),
            ));
            return;
        }
    };

    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            matches!(
                path.extension().and_then(|extension| extension.to_str()),
                Some("md" | "j2")
            )
        })
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        match load_file(&path, source) {
            Ok(template) => {
                selected.insert(template.name.clone(), template);
            }
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
}

fn load_file(path: &Path, source: PromptTemplateSource) -> Result<PromptTemplate, PromptTemplateDiagnostic> {
    let metadata = fs::metadata(path)
        .map_err(|error| PromptTemplateDiagnostic::new(path, format!("failed to inspect prompt template: {error}")))?;
    if !metadata.is_file() {
        return Err(PromptTemplateDiagnostic::new(
            path,
            "prompt template is not a regular file",
        ));
    }
    if metadata.len() > MAX_TEMPLATE_BYTES {
        return Err(PromptTemplateDiagnostic::new(
            path,
            format!("prompt template exceeds the {MAX_TEMPLATE_BYTES}-byte limit"),
        ));
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| PromptTemplateDiagnostic::new(path, format!("failed to read prompt template: {error}")))?;
    parse_template(path, &raw, source, Some(path.to_path_buf()))
}

fn parse_template(
    display_path: &Path, raw: &str, source: PromptTemplateSource, path: Option<PathBuf>,
) -> Result<PromptTemplate, PromptTemplateDiagnostic> {
    let name = display_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|name| valid_template_name(name))
        .ok_or_else(|| {
            PromptTemplateDiagnostic::new(
                display_path,
                "filename must use only ASCII letters, digits, hyphens, and underscores",
            )
        })?
        .to_string();
    let (frontmatter, body) =
        split_frontmatter(raw).map_err(|message| PromptTemplateDiagnostic::new(display_path, message))?;
    let metadata = frontmatter
        .map(serde_yaml_ng::from_str::<Frontmatter>)
        .transpose()
        .map_err(|error| PromptTemplateDiagnostic::new(display_path, format!("invalid YAML frontmatter: {error}")))?
        .unwrap_or_default();
    let description = metadata.description.unwrap_or_else(|| {
        body.lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("prompt template")
            .trim()
            .to_string()
    });
    let description = utils::truncate_ellipsis(&description, 120);

    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    environment.template_from_str(body).map_err(|error| {
        PromptTemplateDiagnostic::new(display_path, format!("invalid MiniJinja template: {error:#}"))
    })?;

    Ok(PromptTemplate {
        name,
        description,
        argument_hint: metadata.argument_hint.filter(|hint| !hint.trim().is_empty()),
        body: body.to_string(),
        source,
        path,
    })
}

fn split_frontmatter(raw: &str) -> Result<(Option<&str>, &str), String> {
    let Some(after_open) = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n")) else {
        return Ok((None, raw));
    };
    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let body_start = offset + line.len();
            return Ok((Some(&after_open[..offset]), &after_open[body_start..]));
        }
        offset += line.len();
    }
    Err("YAML frontmatter is missing its closing `---` line".to_string())
}

fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;

    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            started = true;
            continue;
        }
        if character == '\\' {
            escaped = true;
            started = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else {
                current.push(character);
            }
            started = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            quote = Some(character);
            started = true;
        } else if character.is_whitespace() {
            if started {
                tokens.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(character);
            started = true;
        }
    }

    if escaped {
        return Err("prompt template arguments end with an incomplete escape".to_string());
    }
    if quote.is_some() {
        return Err("prompt template arguments contain an unclosed quote".to_string());
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn valid_template_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn reserved_name(name: &str) -> bool {
    matches!(name, "args" | "arguments" | "named")
        || name
            .strip_prefix("arg")
            .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_positional_and_named_arguments() {
        let parsed = parse_args("one 'two words' owner=hailey title=\"hard bug\"").expect("parse args");

        assert_eq!(parsed.positional, ["one", "two words"]);
        assert_eq!(parsed.named.get("owner").map(String::as_str), Some("hailey"));
        assert_eq!(parsed.named.get("title").map(String::as_str), Some("hard bug"));
    }

    #[test]
    fn treats_urls_and_paths_with_equals_as_positional_arguments() {
        let parsed = parse_args("https://example.test/pull?view=files path/to/a=b.md").expect("parse args");

        assert_eq!(
            parsed.positional,
            ["https://example.test/pull?view=files", "path/to/a=b.md"]
        );
        assert!(parsed.named.is_empty());
    }

    #[test]
    fn rejects_duplicate_and_reserved_named_arguments() {
        assert!(
            parse_args("scope=one scope=two")
                .unwrap_err()
                .contains("more than once")
        );
        assert!(parse_args("args=hidden").unwrap_err().contains("reserved"));
    }

    #[test]
    fn renders_positional_and_named_context() {
        let template = PromptTemplate {
            name: "example".to_string(),
            description: String::new(),
            argument_hint: None,
            body: "{{ arg1 }} / {{ args[1] }} / {{ owner }} / {{ named.owner }} / {{ arguments }}".to_string(),
            source: PromptTemplateSource::Project,
            path: None,
        };

        let rendered = render(&template, "first second owner=stormlight").expect("render template");

        assert_eq!(rendered, "first / second / stormlight / stormlight / first second");
    }

    #[test]
    fn rejects_empty_and_oversized_rendered_prompts() {
        let mut template = PromptTemplate {
            name: "bounded".to_string(),
            description: String::new(),
            argument_hint: None,
            body: "   \n".to_string(),
            source: PromptTemplateSource::Project,
            path: None,
        };

        assert!(render(&template, "").unwrap_err().contains("output is empty"));

        template.body = "x".repeat(MAX_RENDERED_BYTES + 1);
        assert!(render(&template, "").unwrap_err().contains("output exceeds"));
    }

    #[test]
    fn fuel_bounds_expensive_templates() {
        let template = PromptTemplate {
            name: "expensive".to_string(),
            description: String::new(),
            argument_hint: None,
            body: "{% for outer in range(1000) %}{% for inner in range(1000) %}{% set value = outer + inner %}{% endfor %}{% endfor %}done".to_string(),
            source: PromptTemplateSource::Project,
            path: None,
        };

        let error = render(&template, "").unwrap_err();
        assert!(error.contains("fuel"), "unexpected render error: {error}");
    }

    #[test]
    fn bundled_commit_template_is_read_only_and_conventional() {
        let root = tempfile::tempdir().expect("temp workspace");
        let inventory = discover(root.path());
        let commit = inventory
            .templates
            .iter()
            .find(|template| template.name == "commit")
            .expect("commit template");

        assert_eq!(commit.source, PromptTemplateSource::BuiltIn);
        assert!(commit.description.contains("Conventional Commit"));
        assert!(commit.body.contains("describe only the staged changes"));
        assert!(commit.body.contains("<type>[optional scope][!]: <description>"));
        assert!(
            commit
                .body
                .contains("Do not edit files, stage changes, create\na commit")
        );
    }

    #[test]
    fn untrusted_project_templates_are_ignored_without_masking_user_templates() {
        let root = tempfile::tempdir().expect("temp workspace");
        let home = tempfile::tempdir().expect("temp home");
        fs::create_dir_all(home.path().join(".thndrs/prompts")).expect("global prompts");
        fs::create_dir_all(root.path().join(".thndrs/prompts")).expect("project prompts");
        fs::write(home.path().join(".thndrs/prompts/review.md"), "global").expect("global template");
        fs::write(root.path().join(".thndrs/prompts/review.j2"), "project").expect("project template");

        let _guard = crate::test_env::lock();
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.path()) };
        let inventory = discover(root.path());
        unsafe {
            if let Some(old_home) = old_home {
                std::env::set_var("HOME", old_home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        let review = inventory
            .templates
            .iter()
            .find(|template| template.name == "review")
            .expect("user review template");
        assert_eq!(review.body, "global");
        assert_eq!(review.source, PromptTemplateSource::User);
        assert!(
            inventory
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("project prompt templates are inactive"))
        );
    }

    #[test]
    fn project_templates_override_global_and_built_in_templates() {
        let root = tempfile::tempdir().expect("temp workspace");
        let home = tempfile::tempdir().expect("temp home");
        fs::create_dir_all(home.path().join(".thndrs/prompts")).expect("global prompts");
        fs::create_dir_all(root.path().join(".thndrs/prompts")).expect("project prompts");
        fs::write(home.path().join(".thndrs/prompts/review.md"), "global").expect("global template");
        fs::write(root.path().join(".thndrs/prompts/review.j2"), "project").expect("project template");

        let _guard = crate::test_env::lock();
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.path()) };
        let fingerprint = trust::fingerprint_directories(root.path(), &[root.path().join(".thndrs/prompts")])
            .expect("fingerprint")
            .expect("project templates");
        trust::trust_project(root.path(), ProjectTrustScope::PromptTemplates, &fingerprint)
            .expect("trust project templates");
        let inventory = discover(root.path());
        unsafe {
            if let Some(old_home) = old_home {
                std::env::set_var("HOME", old_home);
            } else {
                std::env::remove_var("HOME");
            }
        }
        let review = inventory
            .templates
            .iter()
            .find(|template| template.name == "review")
            .expect("review template");

        assert_eq!(review.body, "project");
        assert_eq!(review.source, PromptTemplateSource::Project);
    }

    #[test]
    fn invalid_template_is_reported_and_skipped() {
        let root = tempfile::tempdir().expect("temp workspace");
        let home = tempfile::tempdir().expect("temp home");
        fs::create_dir_all(root.path().join(".thndrs/prompts")).expect("project prompts");
        fs::write(root.path().join(".thndrs/prompts/broken.j2"), "{{ missing").expect("broken template");

        let _guard = crate::test_env::lock();
        let old_home = std::env::var_os("HOME");
        unsafe { std::env::set_var("HOME", home.path()) };
        let fingerprint = trust::fingerprint_directories(root.path(), &[root.path().join(".thndrs/prompts")])
            .expect("fingerprint")
            .expect("project templates");
        trust::trust_project(root.path(), ProjectTrustScope::PromptTemplates, &fingerprint)
            .expect("trust project templates");
        let inventory = discover(root.path());
        unsafe {
            if let Some(old_home) = old_home {
                std::env::set_var("HOME", old_home);
            } else {
                std::env::remove_var("HOME");
            }
        }

        assert!(!inventory.templates.iter().any(|template| template.name == "broken"));
        assert!(
            inventory
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.path.ends_with("broken.j2"))
        );
    }
}
