//! Prompt inspection, diagnostics, and runtime observability.

use super::*;
pub(crate) use crate::prompt::PromptBundle;
#[cfg(test)]
pub(crate) use crate::prompt::{EnvironmentMetadata, HistoryReuse, default_fragments};

/// Render the `--print-prompt` debug view as a string.
///
/// Produces a human-readable dump of the assembled prompt bundle: system prompt,
/// tool catalog, lowered provider messages, and environment metadata. Secrets
/// (`sk-` prefixed values) are redacted. The date is replaced with `[date]` so
/// the output is stable for snapshot testing.
pub fn render_print_prompt(bundle: &PromptBundle) -> String {
    let system_prompt = crate::prompt::render_system_prompt(bundle);
    let messages = crate::prompt::lower_to_provider_messages(bundle);
    let tool_catalog = crate::prompt::render_tool_catalog(bundle);
    let mut out = String::new();

    out.push_str(&format!(
        "=== System Prompt ===
{system_prompt}

"
    ));
    out.push_str(&format!(
        "=== Tool Catalog ({} tools) ===
",
        bundle.tool_catalog.len()
    ));
    out.push_str(&serde_json::to_string_pretty(&tools::sorted_json_value(&tool_catalog)).unwrap_or_default());
    out.push_str(&format!(
        "

=== Lowered Provider Messages ({} messages) ===
",
        messages.len()
    ));

    for (i, msg) in messages.iter().enumerate() {
        let redacted = redact_secret(&msg.as_text());
        let truncated = if redacted.len() > 200 { format!("{}...", &redacted[..200]) } else { redacted };
        out.push_str(&format!(
            "[{i}] {}: {truncated}
",
            msg.role
        ));
    }

    out.push_str(
        "
=== Environment ===
",
    );
    out.push_str(&format!(
        "  cwd: {}
",
        bundle.environment.cwd
    ));
    out.push_str(&format!(
        "  model: {}
",
        bundle.environment.model
    ));
    out.push_str(
        "  date: [date]
",
    );
    out.push_str(&format!(
        "  context_sources: {}
",
        bundle.project_context.len()
    ));
    out.push_str(&format!(
        "  skills: {}
",
        bundle.available_skills.len()
    ));

    out
}

/// Initialize durable session tracing when the current run allows local logs.
pub(crate) fn init_tracing(
    workspace_root: &Path, session_id: &str, run_persistence: app::RunPersistence,
) -> Option<Observability> {
    if run_persistence.is_ephemeral() {
        return None;
    }

    let session_log_dir = workspace_root.join(".thndrs").join("logs").join("sessions");
    let daily_log_dir = workspace_root.join(".thndrs").join("logs").join("daily");
    let session_log_path = session_log_dir.join(format!("thndrs-{session_id}.log"));
    let daily_log_path = daily_log_dir.join(format!("{}.log", datetime::rounded_date()));
    std::fs::create_dir_all(&session_log_dir).ok()?;
    std::fs::create_dir_all(&daily_log_dir).ok()?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&session_log_path)
        .ok()?;

    // The tracing subscriber is process-global, so a later `/new` or `/resume`
    // cannot replace its writer. Keep the per-session paths even when it was
    // already initialized: daily lifecycle events must still follow the active
    // session.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .try_init();
    Some(Observability { session_log_path, daily_log_path })
}

pub(crate) fn daily_detail_value(value: &str) -> String {
    value.chars().filter(|c| *c != '\n' && *c != '\r').take(300).collect()
}

pub(crate) fn append_daily_log(observability: &Option<Observability>, session_id: &str, event: &str, details: &str) {
    let Some(obs) = observability else {
        return;
    };

    if let Some(parent) = obs.daily_log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    use std::io::Write;
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&obs.daily_log_path)
    else {
        return;
    };
    let _ = writeln!(
        file,
        "{} session={} event={} {}",
        datetime::now_iso8601(),
        session_id,
        event,
        details
    );
}

/// Print the assembled prompt bundle with secrets redacted, without calling
/// the provider. This is the `--print-prompt` debug path.
pub(crate) fn run_print_prompt(cli: &Cli) -> io::Result<()> {
    let workspace_root = crate::context::discover_workspace_root(&cli.cwd);
    let context_sources = crate::context::discover_instructions(&workspace_root).sources;
    let skill_inventory = skills::discover(&workspace_root, &cli.skill_dirs);
    let mcp_manager = super::load_mcp_manager_for_workspace(&workspace_root).ok();
    let tool_catalog = tools::runtime_tool_definitions(mcp_manager.as_deref());
    let user_turn = String::from("(no user prompt — print-prompt debug mode)");
    let provider = acp::config::provider_label(&cli.model);
    let (limits, _) = agent_context::ModelContextLimits::resolve(provider, &cli.model, None, None);
    let selection = agent_context::SelectionInput {
        harness: crate::prompt::default_fragments()
            .into_iter()
            .map(|fragment| agent_context::HarnessCandidate::new(fragment.name, fragment.content.len()))
            .collect(),
        user_turn: Some(agent_context::UserTurnCandidate::new(
            "print-prompt",
            1,
            user_turn.len(),
        )),
        instructions: context_sources
            .iter()
            .map(|source| agent_context::InstructionCandidate {
                path: source.path.clone(),
                scope: source.scope.clone(),
                content_hash: source.content_hash,
                byte_count: source.byte_count,
                content: Some(source.content.clone()),
                truncated: source.truncated,
                applicable: true,
            })
            .collect(),
        ..Default::default()
    };
    let ledger = agent_context::select_context(&selection, limits);

    let bundle = PromptBundle::new_with_skills(
        &workspace_root,
        &cli.model,
        &context_sources,
        &skill_inventory.skills,
        &[],
        &user_turn,
    )
    .with_tool_catalog(tool_catalog)
    .with_context_ledger(ledger);

    let mut output = render_print_prompt(&bundle);
    output.push_str(&render_print_prompt_config(cli, &workspace_root));
    print!("{output}");
    Ok(())
}

pub(crate) fn render_print_prompt_config(cli: &Cli, workspace_root: &Path) -> String {
    let session_dir = cli
        .session_dir
        .clone()
        .unwrap_or_else(|| session::sessions_dir(workspace_root));
    let mut out = String::new();

    out.push_str(
        "

=== Effective Config ===
",
    );
    out.push_str(&format!(
        "  provider: {}
",
        provider_label(&cli.model)
    ));
    out.push_str(&format!(
        "  model: {}
",
        cli.model
    ));
    out.push_str(&format!(
        "  workspace: {}
",
        workspace_root.display()
    ));
    out.push_str(&format!(
        "  session_dir: {}
",
        session_dir.display()
    ));

    out.push_str(
        "  files:
",
    );
    if cli.config_layers.is_empty() {
        out.push_str(
            "    none
",
        );
    } else {
        for layer in &cli.config_layers {
            let path = layer.display_path.as_deref().unwrap_or("<unknown>");
            let hash = layer.hash.as_deref().unwrap_or("");
            out.push_str(&format!(
                "    {} {} {}
",
                layer.source.as_str(),
                path,
                hash
            ));
        }
    }

    out.push_str(
        "  origins:
",
    );
    if cli.config_origins.is_empty() {
        out.push_str(
            "    none
",
        );
    } else {
        for (key, origin) in &cli.config_origins {
            out.push_str(&format!(
                "    {key}: {}:{}
",
                origin.source.as_str(),
                origin.detail
            ));
        }
    }

    out.push_str(
        "  diagnostics:
",
    );
    if cli.config_diagnostics.is_empty() {
        out.push_str("    none");
    } else {
        for diagnostic in &cli.config_diagnostics {
            out.push_str(&format!(
                "    {}
",
                redact_secret(diagnostic)
            ));
        }
    }

    out
}

/// Redact secret-like values from prompt content for debug display.
pub(crate) fn redact_secret(text: &str) -> String {
    text.replace("sk-", "sk-[REDACTED]")
}

pub(crate) fn session_resume_message(app: &App) -> Option<String> {
    (!app.is_ephemeral()).then(|| {
        format!(
            "Session ID: {}
Resume with: thndrs session resume {}",
            app.session.id, app.session.id
        )
    })
}
