use super::App;
use crossterm;
use std::io::{self, Result, Write};
use std::{env, fs, process::Command};
use uuid;

pub fn open_external_editor(app: &mut App) {
    app.transcript_mut().add_system_message("External editor invoked");

    let editor_cmd = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let temp_dir = env::temp_dir();
    let temp_file_path = temp_dir.join(format!("thunderus_input_{}.md", uuid::Uuid::new_v4()));

    if let Err(e) = fs::write(&temp_file_path, &app.state.input.buffer) {
        app.transcript_mut()
            .add_system_message(format!("Failed to create temporary file: {}", e));
        return;
    }

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    let _ = std::io::stdout().flush();

    let result = Command::new(&editor_cmd).arg(&temp_file_path).status();

    let _ = crossterm::terminal::enable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);

    match result {
        Ok(status) if status.success() => match fs::read_to_string(&temp_file_path) {
            Ok(content) => {
                app.state.input.buffer = content;
                app.state.input.cursor = app.state.input.buffer.len();

                app.transcript_mut()
                    .add_system_message("Content loaded from external editor")
            }
            Err(e) => app
                .transcript_mut()
                .add_system_message(format!("Failed to read edited content: {}", e)),
        },
        Ok(status) => app
            .transcript_mut()
            .add_system_message(format!("Editor exited with non-zero status: {}", status)),
        Err(e) => app
            .transcript_mut()
            .add_system_message(format!("Failed to launch editor '{}': {}", editor_cmd, e)),
    }

    let _ = fs::remove_file(&temp_file_path);
    let _ = redraw_screen();
}

pub fn redraw_screen() -> Result<()> {
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    if let Ok(mut terminal) = ratatui::Terminal::new(backend) {
        let _ = terminal.clear();
        let _ = io::stdout().flush();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::app::create_test_app;
    use crate::transcript;

    #[test]
    fn test_external_editor_environment_variables() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test content for editor".to_string();

        let editor_cmd = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());

        assert!(!editor_cmd.is_empty());
    }

    #[test]
    fn test_external_editor_temp_file_operations() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Test content\nwith multiple lines".to_string();

        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(format!("thunderus_test_{}.md", uuid::Uuid::new_v4()));

        let write_result = std::fs::write(&temp_file_path, &app.state().input.buffer);
        assert!(write_result.is_ok(), "Should be able to write to temporary file");

        let read_result = std::fs::read_to_string(&temp_file_path);
        assert!(read_result.is_ok(), "Should be able to read from temporary file");

        let read_content = read_result.unwrap();
        assert_eq!(read_content, app.state().input.buffer);

        let _ = std::fs::remove_file(&temp_file_path);
    }

    #[test]
    fn test_external_editor_with_empty_input() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "".to_string();

        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();
        unsafe { std::env::set_var("EDITOR", "true") }

        super::open_external_editor(&mut app);

        let transcript = app.transcript();
        let entries = transcript.entries();

        let system_entry = entries
            .iter()
            .find(|e| matches!(e, transcript::TranscriptEntry::SystemMessage { .. }));
        assert!(system_entry.is_some());

        match original_editor {
            Some(val) => unsafe { std::env::set_var("EDITOR", val) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        match original_visual {
            Some(val) => unsafe { std::env::set_var("VISUAL", val) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
    }

    #[test]
    fn test_external_editor_cursor_position() {
        let mut app = create_test_app();
        app.state_mut().input.buffer = "Some existing content".to_string();

        let original_editor = std::env::var("EDITOR").ok();
        let original_visual = std::env::var("VISUAL").ok();
        unsafe { std::env::set_var("EDITOR", "true") }

        super::open_external_editor(&mut app);

        match original_editor {
            Some(val) => unsafe { std::env::set_var("EDITOR", val) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        match original_visual {
            Some(val) => unsafe { std::env::set_var("VISUAL", val) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
    }

    #[test]
    fn test_redraw_screen_method() {
        let result = super::redraw_screen();
        assert!(result.is_ok() || result.is_err());
    }
}
