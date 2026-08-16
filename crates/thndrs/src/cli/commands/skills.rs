//! Skill inspection command definitions.

use std::io::{self, Write};

use clap::Subcommand;

use crate::cli::Cli;
use crate::{context, skills};

/// Skill inspection commands.
#[derive(Clone, Debug, Eq, PartialEq, Subcommand)]
pub enum SkillsCommand {
    /// Report duplicate skill names and their deterministic resolution.
    Doctor,
}

/// Run a skill inspection command.
pub fn run(cli: &Cli, command: &SkillsCommand) -> io::Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    run_with_writer(cli, command, &mut lock)
}

/// Run a skill inspection command with an injected writer.
pub fn run_with_writer<W: Write>(cli: &Cli, command: &SkillsCommand, writer: &mut W) -> io::Result<()> {
    match command {
        SkillsCommand::Doctor => run_doctor(cli, writer),
    }
}

fn run_doctor<W: Write>(cli: &Cli, writer: &mut W) -> io::Result<()> {
    let workspace = context::discover_workspace_root(&cli.cwd);
    let inventory = skills::discover(&workspace, &cli.skill_dirs);

    writeln!(writer, "thndrs skills doctor")?;
    writeln!(
        writer,
        "precedence: the first matching skill in discovery-root order is selected"
    )?;
    if inventory.duplicates.is_empty() {
        writeln!(writer, "duplicates: <none>")?;
    } else {
        writeln!(writer, "duplicates:")?;
        for duplicate in inventory.duplicates {
            writeln!(writer, "  {}:", duplicate.name)?;
            writeln!(writer, "    selected: {}", duplicate.selected_path.display())?;
            for path in duplicate.ignored_paths {
                writeln!(writer, "    ignored: {}", path.display())?;
            }
        }
    }
    if !inventory.diagnostics.is_empty() {
        writeln!(writer, "diagnostics:")?;
        for diagnostic in inventory.diagnostics {
            writeln!(writer, "  - {}", diagnostic.summary())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Command;
    use std::io::Cursor;

    #[test]
    fn doctor_reports_duplicate_skills_without_treating_them_as_errors() {
        let workspace = tempfile::tempdir().expect("temp workspace");
        let path = workspace.path();
        for root in [".agents/skills", ".claude/skills"] {
            let skill = path.join(root).join("example-skill/SKILL.md");
            std::fs::create_dir_all(skill.parent().expect("skill parent")).expect("create skill parent");
            std::fs::write(skill, "---\nname: example-skill\ndescription: Helps.\n---\n# Skill\n")
                .expect("write skill");
        }

        let cli = Cli::try_parse_from([
            "thndrs",
            "--cwd",
            path.to_str().expect("workspace path"),
            "skills",
            "doctor",
        ])
        .expect("parse");
        let command = match &cli.command {
            Some(Command::Skills { command }) => command,
            _ => panic!("expected skills doctor command"),
        };
        let mut output = Cursor::new(Vec::new());
        run_with_writer(&cli, command, &mut output).expect("run doctor");
        let output = String::from_utf8(output.into_inner()).expect("UTF-8 output");

        assert!(output.contains("thndrs skills doctor"));
        assert!(output.contains("duplicates:"));
        assert!(output.contains("example-skill:"));
        assert!(output.contains("selected:"));
        assert!(output.contains("ignored:"));
        assert!(!output.contains("blocked by trust"));
    }
}
