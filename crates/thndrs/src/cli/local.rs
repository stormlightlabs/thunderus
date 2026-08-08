//! Explicit local-user actions launched from the interactive composer.
//!
//! Direct commands are parsed into argv and never passed through a shell.
//! External editors temporarily own the terminal and edit a bounded temporary
//! prompt file.

use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

use tempfile::NamedTempFile;

use crate::tools::shell::{self, ProcessKind, ProcessResult, ShellArgs};
use thndrs_agent::CancelToken;

/// Run an explicit composer command with the user's local permissions.
pub fn run_direct_command(command: &str, cwd: &Path, cancel: &CancelToken) -> Result<ProcessResult, String> {
    let argv = parse_argv(command)?;
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| String::from("enter a command after `!`"))?;
    shell::run_command(
        &ShellArgs {
            program: program.clone(),
            args: args.to_vec(),
            cwd: None,
            timeout: None,
            kind: ProcessKind::OneShot,
        },
        cwd,
        cancel,
    )
}

/// Open the current composer text in `$VISUAL` or `$EDITOR` and return it.
pub fn edit_prompt(initial: &str) -> io::Result<String> {
    let editor = env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var("EDITOR").ok().filter(|value| !value.trim().is_empty()))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "set VISUAL or EDITOR to edit the prompt"))?;
    let argv = parse_argv(&editor).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the editor command is empty"))?;

    let file = NamedTempFile::new()?;
    fs::write(file.path(), initial)?;
    let status = Command::new(program).args(args).arg(file.path()).status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "editor exited with status {}",
            status
                .code()
                .map_or_else(|| String::from("aborted"), |code| code.to_string())
        )));
    }
    fs::read_to_string(file.path())
}

/// Split a command into argv with small, predictable quote and escape rules.
///
/// This is not a shell parser: expansion, redirects, pipes, and substitutions
/// stay ordinary argument text. Single and double quotes group whitespace;
/// backslash escapes the next character outside single quotes.
pub fn parse_argv(input: &str) -> Result<Vec<String>, String> {
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum Quote {
        None,
        Single,
        Double,
    }

    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quote = Quote::None;
    let mut escaped = false;
    let mut started = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            started = true;
            continue;
        }
        match (quote, ch) {
            (Quote::None | Quote::Double, '\\') => {
                escaped = true;
                started = true;
            }
            (Quote::None, '\'') => {
                quote = Quote::Single;
                started = true;
            }
            (Quote::Single, '\'') => quote = Quote::None,
            (Quote::None, '"') => {
                quote = Quote::Double;
                started = true;
            }
            (Quote::Double, '"') => quote = Quote::None,
            (Quote::None, ch) if ch.is_whitespace() => {
                if started {
                    argv.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            (_, ch) => {
                current.push(ch);
                started = true;
            }
        }
    }

    if escaped {
        return Err(String::from("command ends with an incomplete escape"));
    }
    if quote != Quote::None {
        return Err(String::from("command contains an unclosed quote"));
    }
    if started {
        argv.push(current);
    }
    Ok(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_parser_groups_quotes_and_escapes_whitespace() {
        assert_eq!(
            parse_argv(r#"cargo test "two words" 'three words' four\ five """#),
            Ok(vec![
                String::from("cargo"),
                String::from("test"),
                String::from("two words"),
                String::from("three words"),
                String::from("four five"),
                String::new(),
            ])
        );
    }

    #[test]
    fn argv_parser_rejects_incomplete_input() {
        assert_eq!(
            parse_argv("echo 'open"),
            Err(String::from("command contains an unclosed quote"))
        );
        assert_eq!(
            parse_argv("echo trailing\\"),
            Err(String::from("command ends with an incomplete escape"))
        );
    }

    #[test]
    fn argv_parser_does_not_interpret_shell_operators() {
        assert_eq!(
            parse_argv("echo hi | tee out"),
            Ok(vec!["echo", "hi", "|", "tee", "out"]
                .into_iter()
                .map(String::from)
                .collect())
        );
    }
}
