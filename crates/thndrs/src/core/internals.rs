//! Compact self-knowledge snapshot for model and startup inspection.
//!
//! This module owns the small, stable shape that describes what `thndrs`
//! knows about itself for the current run.

use crate::agent::ProviderKind;
use crate::context::ContextSource;
use crate::prompt::PromptBundle;
use crate::skills::SkillMetadata;
use crate::utils;
use thndrs_agent::context::render_model_dashboard;

pub const RENDERER_MODE: &str = "direct-inline";

const DOCUMENTATION_MAP: &[DocumentationEntry] = &[
    DocumentationEntry { topic: "CLI", path: "docs/src/content/docs/docs/reference/cli.md" },
    DocumentationEntry { topic: "configuration", path: "docs/src/content/docs/docs/reference/configuration.md" },
    DocumentationEntry { topic: "sessions", path: "docs/src/content/docs/docs/reference/session-format.md" },
    DocumentationEntry { topic: "tool boundary", path: "docs/src/content/docs/docs/concepts/tool-boundary.md" },
    DocumentationEntry { topic: "tools", path: "docs/src/content/docs/docs/reference/tools.md" },
    DocumentationEntry { topic: "web search and URL reading", path: "docs/src/content/docs/docs/usage/web-search.md" },
    DocumentationEntry { topic: "prompt assembly", path: "docs/src/content/docs/docs/concepts/prompt-assembly.md" },
    DocumentationEntry { topic: "project context", path: "docs/src/content/docs/docs/usage/project-context.md" },
    DocumentationEntry { topic: "skills", path: "docs/src/content/docs/docs/usage/skills.md" },
    DocumentationEntry { topic: "OpenCode Go provider", path: "docs/src/content/docs/docs/providers/opencode-go.md" },
    DocumentationEntry { topic: "OpenCode Zen provider", path: "docs/src/content/docs/docs/providers/opencode-zen.md" },
    DocumentationEntry { topic: "ChatGPT Codex provider", path: "docs/src/content/docs/docs/providers/chatgpt.md" },
    DocumentationEntry { topic: "renderer", path: "docs/src/content/docs/docs/usage/tui.md" },
    DocumentationEntry { topic: "development workflow", path: "docs/src/content/docs/docs/development/workflow.md" },
];

const CAPABILITIES: &[&str] = &[
    "structured prompt bundle",
    "bounded workspace file tools",
    "provider-native tool schemas",
    "agent skills metadata",
    "append-only JSONL sessions",
    "MCP-configured web search",
    "URL/article reading",
    "direct inline terminal renderer",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocumentationEntry {
    pub topic: &'static str,
    pub path: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSnapshot {
    pub path: String,
    pub scope: String,
    pub content_hash: u64,
    pub truncated: bool,
    pub byte_count: usize,
}

impl ContextSnapshot {
    fn from_source(source: &ContextSource) -> Self {
        Self {
            path: source.path.display().to_string(),
            scope: source.scope.clone(),
            content_hash: source.content_hash,
            truncated: source.truncated,
            byte_count: source.byte_count,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillSnapshot {
    pub name: String,
    pub path: String,
    pub source: String,
}

impl SkillSnapshot {
    fn from_metadata(skill: &SkillMetadata) -> Self {
        Self {
            name: skill.name.clone(),
            path: skill.path.display().to_string(),
            source: skill.source.label().to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppIdentitySnapshot {
    pub app_name: &'static str,
    pub app_version: &'static str,
    pub capabilities: Vec<&'static str>,
}

impl Default for AppIdentitySnapshot {
    fn default() -> Self {
        Self { app_name: "thndrs", app_version: env!("CARGO_PKG_VERSION"), capabilities: CAPABILITIES.to_vec() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSnapshot {
    pub provider: String,
    pub model: String,
    pub url_reader: String,
}

impl ProviderSnapshot {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            url_reader: "read_url fetches public HTTP(S) and extracts HTML with Lectito".to_string(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeSnapshot {
    pub provider: ProviderSnapshot,
    pub workspace: String,
    pub renderer_mode: String,
    pub tools: Vec<String>,
}

impl RuntimeSnapshot {
    pub fn new(
        provider: ProviderSnapshot, ws: impl Into<String>, rmode: impl Into<String>, tools: Vec<String>,
    ) -> Self {
        Self { provider, workspace: ws.into(), renderer_mode: rmode.into(), tools }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptContextSnapshot {
    pub prompt_fragments: Vec<String>,
    pub context_sources: Vec<ContextSnapshot>,
}

impl PromptContextSnapshot {
    pub fn new(fragments: Vec<String>, ctx: &[ContextSource]) -> Self {
        Self { prompt_fragments: fragments, context_sources: ctx.iter().map(ContextSnapshot::from_source).collect() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferenceSnapshot {
    pub docs: Vec<DocumentationEntry>,
    pub skills: Vec<SkillSnapshot>,
}

impl ReferenceSnapshot {
    pub fn from_skills(skills: &[SkillMetadata]) -> Self {
        Self { docs: DOCUMENTATION_MAP.to_vec(), skills: skills.iter().map(SkillSnapshot::from_metadata).collect() }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeInventorySnapshot {
    pub references: ReferenceSnapshot,
    pub prompt_context: PromptContextSnapshot,
}

impl KnowledgeInventorySnapshot {
    pub fn new(refs: ReferenceSnapshot, ctx: PromptContextSnapshot) -> Self {
        Self { references: refs, prompt_context: ctx }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelfKnowledgeSnapshot {
    pub identity: AppIdentitySnapshot,
    pub runtime: RuntimeSnapshot,
    pub inventory: KnowledgeInventorySnapshot,
    pub diagnostics: Vec<String>,
    /// Compact context dashboard from the context selection ledger,
    /// when a projection is attached to the prompt bundle.
    pub context_dashboard: Option<String>,
}

impl From<&PromptBundle> for SelfKnowledgeSnapshot {
    fn from(bundle: &PromptBundle) -> SelfKnowledgeSnapshot {
        let provider = ProviderSnapshot::new(
            ProviderKind::for_model(&bundle.environment.model).label(),
            &bundle.environment.model,
        );
        let runtime = RuntimeSnapshot::new(
            provider,
            bundle.environment.cwd.clone(),
            RENDERER_MODE,
            bundle.tool_catalog.iter().map(|tool| tool.name.to_string()).collect(),
        );
        let references = ReferenceSnapshot::from_skills(&bundle.available_skills);
        let prompt_context = PromptContextSnapshot::new(
            bundle
                .fragments
                .iter()
                .map(|fragment| fragment.name.to_string())
                .collect(),
            &bundle.project_context,
        );
        let inventory = KnowledgeInventorySnapshot::new(references, prompt_context);
        let context_dashboard = bundle.context_ledger.as_ref().map(render_model_dashboard);
        let snapshot = SelfKnowledgeSnapshot::new(AppIdentitySnapshot::default(), runtime, inventory, Vec::new());
        if let Some(dashboard) = context_dashboard {
            snapshot.with_context_dashboard(dashboard)
        } else {
            snapshot
        }
    }
}

impl SelfKnowledgeSnapshot {
    pub fn new(
        identity: AppIdentitySnapshot, runtime: RuntimeSnapshot, inventory: KnowledgeInventorySnapshot,
        diagnostics: Vec<String>,
    ) -> Self {
        Self { identity, runtime, inventory, diagnostics, context_dashboard: None }
    }

    /// Attach a compact context dashboard string rendered from the context selection ledger.
    pub fn with_context_dashboard(mut self, dashboard: impl Into<String>) -> Self {
        self.context_dashboard = Some(dashboard.into());
        self
    }

    pub fn render_model_visible(&self) -> String {
        let mut out = String::new();
        out.push_str("<thndrs_self_knowledge>\n");
        out.push_str("  <self_description>\n");
        element(&mut out, 4, "name", self.identity.app_name);
        element(&mut out, 4, "version", self.identity.app_version);
        out.push_str("    <capabilities>\n");
        for capability in &self.identity.capabilities {
            element(&mut out, 6, "capability", capability);
        }
        out.push_str("    </capabilities>\n");
        out.push_str("  </self_description>\n");

        out.push_str("  <docs_map>\n");
        for doc in &self.inventory.references.docs {
            out.push_str("    <doc>\n");
            element(&mut out, 6, "topic", doc.topic);
            element(&mut out, 6, "path", doc.path);
            out.push_str("    </doc>\n");
        }
        out.push_str("  </docs_map>\n");

        out.push_str("  <runtime_state>\n");
        element(&mut out, 4, "workspace", &self.runtime.workspace);
        element(&mut out, 4, "renderer_mode", &self.runtime.renderer_mode);
        out.push_str("    <provider>\n");
        element(&mut out, 6, "name", &self.runtime.provider.provider);
        element(&mut out, 6, "model", &self.runtime.provider.model);
        element(&mut out, 6, "url_reader", &self.runtime.provider.url_reader);
        out.push_str("    </provider>\n");

        out.push_str("    <tools>\n");
        for tool in &self.runtime.tools {
            element(&mut out, 6, "tool", tool);
        }
        out.push_str("    </tools>\n");

        out.push_str("    <prompt_fragments>\n");
        for fragment in &self.inventory.prompt_context.prompt_fragments {
            element(&mut out, 6, "fragment", fragment);
        }
        out.push_str("    </prompt_fragments>\n");

        out.push_str("    <project_context>\n");
        for source in &self.inventory.prompt_context.context_sources {
            out.push_str("      <source>\n");
            element(&mut out, 8, "path", &source.path);
            element(&mut out, 8, "scope", &source.scope);
            element(&mut out, 8, "hash", &source.content_hash.to_string());
            element(&mut out, 8, "truncated", &source.truncated.to_string());
            element(&mut out, 8, "byte_count", &source.byte_count.to_string());
            out.push_str("      </source>\n");
        }
        out.push_str("    </project_context>\n");

        out.push_str("    <skills>\n");
        for skill in &self.inventory.references.skills {
            out.push_str("      <skill>\n");
            element(&mut out, 8, "name", &skill.name);
            element(&mut out, 8, "source", &skill.source);
            element(&mut out, 8, "path", &skill.path);
            out.push_str("      </skill>\n");
        }
        out.push_str("    </skills>\n");

        if let Some(dashboard) = &self.context_dashboard {
            out.push_str("    ");
            out.push_str(dashboard);
            out.push('\n');
        }

        out.push_str("    <diagnostics>\n");
        for diagnostic in &self.diagnostics {
            element(&mut out, 6, "diagnostic", diagnostic);
        }
        out.push_str("    </diagnostics>\n");
        out.push_str("  </runtime_state>\n");
        out.push_str("</thndrs_self_knowledge>");
        out
    }

    pub fn startup_sections(&self) -> Vec<StartupSection> {
        vec![
            StartupSection::new(
                "Runtime",
                vec![
                    format!("provider = \"{}\"", self.runtime.provider.provider),
                    format!("model = \"{}\"", self.runtime.provider.model),
                ],
            ),
            StartupSection::new(
                "Context",
                context_startup_lines(&self.inventory.prompt_context.context_sources),
            ),
            StartupSection::new("Web", vec![self.runtime.provider.url_reader.clone()]),
            StartupSection::new("Skills", {
                let names = self
                    .inventory
                    .references
                    .skills
                    .iter()
                    .map(|skill| skill.name.as_str())
                    .collect::<Vec<&str>>();
                vec![if names.is_empty() { "(none)".to_string() } else { names.join(", ") }]
            }),
            StartupSection::new(
                "Diagnostics",
                if self.diagnostics.is_empty() { vec!["(none)".to_string()] } else { self.diagnostics.clone() },
            ),
        ]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupSection {
    /// TOML-style section heading shown in the startup banner.
    pub heading: &'static str,
    /// Preformatted display lines shown under the section heading.
    pub lines: Vec<String>,
}

impl StartupSection {
    fn new(heading: &'static str, lines: Vec<String>) -> Self {
        Self { heading, lines }
    }
}

fn context_startup_lines(sources: &[ContextSnapshot]) -> Vec<String> {
    if sources.is_empty() {
        vec!["(none)".to_string()]
    } else {
        sources
            .iter()
            .map(|source| match source.truncated {
                true => format!("{} (truncated, {} bytes)", source.path, source.byte_count),
                false => source.path.clone(),
            })
            .collect()
    }
}

fn element(out: &mut String, indent: usize, name: &str, value: &str) {
    out.push_str(&" ".repeat(indent));
    out.push('<');
    out.push_str(name);
    out.push('>');
    out.push_str(&utils::escape_xml(value));
    out.push_str("</");
    out.push_str(name);
    out.push_str(">\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{SkillDiagnostic, SkillSource};
    use crate::tools::ToolDefinition;

    fn test_skill() -> SkillMetadata {
        SkillMetadata {
            name: "inspect".to_string(),
            description: "Inspect project state.".to_string(),
            path: "/repo/.thndrs/skills/inspect/SKILL.md".into(),
            root: "/repo/.thndrs/skills/inspect".into(),
            content_hash: 7,
            byte_count: 100,
            source: SkillSource::Project,
            allowed_tools: Vec::new(),
            license: None,
            compatibility: None,
            metadata: None,
            references: Vec::new(),
        }
    }

    fn test_snapshot(
        model: &str, prompt_fragments: Vec<String>, context_sources: &[ContextSource], tools: &[ToolDefinition],
        skills: &[SkillMetadata], diagnostics: &[SkillDiagnostic],
    ) -> SelfKnowledgeSnapshot {
        let provider = ProviderSnapshot::new("opencode-zen", model);
        let runtime = RuntimeSnapshot::new(
            provider,
            "/repo",
            RENDERER_MODE,
            tools.iter().map(|tool| tool.name.to_string()).collect(),
        );
        let references = ReferenceSnapshot::from_skills(skills);
        let prompt_context = PromptContextSnapshot::new(prompt_fragments, context_sources);
        let inventory = KnowledgeInventorySnapshot::new(references, prompt_context);
        let diagnostics = diagnostics.iter().map(SkillDiagnostic::summary).collect();
        SelfKnowledgeSnapshot::new(AppIdentitySnapshot::default(), runtime, inventory, diagnostics)
    }

    #[test]
    fn model_visible_snapshot_contains_docs_and_runtime_state() {
        let source = ContextSource {
            path: "/repo/AGENTS.md".into(),
            scope: ".".to_string(),
            content: "# Project".to_string(),
            content_hash: 42,
            truncated: false,
            byte_count: 9,
        };
        let diagnostic = SkillDiagnostic { path: "/repo/bad/SKILL.md".into(), message: "invalid".to_string() };
        let snapshot = test_snapshot(
            "test-model",
            vec!["base_identity".to_string(), "self_knowledge".to_string()],
            &[source],
            &crate::tools::tool_definitions(),
            &[test_skill()],
            &[diagnostic],
        );
        let rendered = snapshot.render_model_visible();

        assert!(rendered.contains("<thndrs_self_knowledge>"));
        assert!(rendered.contains("<name>opencode-zen</name>"));
        assert!(rendered.contains("<renderer_mode>direct-inline</renderer_mode>"));
        assert!(rendered.contains("docs/src/content/docs/docs/reference/cli.md"));
        assert!(rendered.contains("<fragment>base_identity</fragment>"));
        assert!(rendered.contains("<tool>read_file_range</tool>"));
        assert!(rendered.contains("<name>inspect</name>"));
        assert!(rendered.contains("skill diagnostic"));
        assert!(
            !rendered.contains("# Project"),
            "snapshot must not include AGENTS.md content"
        );
    }

    #[test]
    fn startup_sections_use_compact_labels() {
        let snapshot = test_snapshot("test-model", vec!["base_identity".to_string()], &[], &[], &[], &[]);
        let sections = snapshot.startup_sections();
        assert!(sections.iter().any(|section| section.heading == "Runtime"));
        let context = sections
            .iter()
            .find(|section| section.heading == "Context")
            .expect("Context section should exist");
        assert!(
            context.lines.iter().any(|line| line == "(none)"),
            "Context with no sources should show (none): {:?}",
            context.lines
        );

        let runtime = sections
            .iter()
            .find(|section| section.heading == "Runtime")
            .expect("Runtime section should exist");
        assert!(
            runtime.lines.iter().any(|line| line.starts_with("provider =")),
            "Runtime section should have provider = ... line: {:?}",
            runtime.lines
        );
    }
}
