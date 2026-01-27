use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::path::PathBuf;
use thunderus_core::trajectory::TrajectoryNode;
use thunderus_core::{ApprovalMode, Event, LoggedEvent, ProviderConfig, SandboxMode};
use thunderus_ui::{components::Inspector, state::AppState};

#[test]
fn test_inspector_render_empty() {
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = create_test_state();

    terminal
        .draw(|f| {
            let inspector = Inspector::new(&state);
            let area = f.area();
            let chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Horizontal)
                .constraints([
                    ratatui::layout::Constraint::Percentage(30),
                    ratatui::layout::Constraint::Percentage(70),
                ])
                .split(area);
            inspector.render(f, chunks[0], chunks[1]);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let content = buffer_to_string(buffer);
    assert!(content.contains("Chain of Evidence"));
}

#[test]
fn test_inspector_render_with_data() {
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = create_test_state();

    let nodes = vec![
        TrajectoryNode {
            event: LoggedEvent {
                seq: 0,
                session_id: "test".to_string(),
                timestamp: "T1".to_string(),
                event: Event::UserMessage { content: "Fix bug".to_string() },
            },
            impact: "Input".to_string(),
            causal_links: vec![1],
        },
        TrajectoryNode {
            event: LoggedEvent {
                seq: 1,
                session_id: "test".to_string(),
                timestamp: "T2".to_string(),
                event: Event::Patch {
                    name: "fix.patch".to_string(),
                    status: thunderus_core::session::events::PatchStatus::Applied,
                    files: vec!["lib.rs".to_string()],
                    diff: "--- a/lib.rs\n+++ b/lib.rs\n@@ -1,1 +1,1 @@\n-old\n+new".to_string(),
                },
            },
            impact: "Change".to_string(),
            causal_links: vec![0],
        },
    ];

    state.evidence.set_nodes(nodes);

    terminal
        .draw(|f| {
            let inspector = Inspector::new(&state);
            let area = f.area();
            let chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Horizontal)
                .constraints([
                    ratatui::layout::Constraint::Percentage(30),
                    ratatui::layout::Constraint::Percentage(70),
                ])
                .split(area);
            inspector.render(f, chunks[0], chunks[1]);
        })
        .unwrap();

    let content = buffer_to_string(terminal.backend().buffer());
    assert!(content.contains("Chain of Evidence"));
    assert!(content.contains("Fix bug"));
    assert!(content.contains("Patch: fix.patch"));
}

fn create_test_state() -> AppState {
    AppState::new(
        PathBuf::from("."),
        "test".to_string(),
        ProviderConfig::Glm {
            api_key: "test".to_string(),
            model: "glm-4.7".to_string(),
            base_url: "https://api.example.com".to_string(),
            thinking: Default::default(),
            options: Default::default(),
        },
        ApprovalMode::Auto,
        SandboxMode::Policy,
        false,
    )
}

fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
    let mut s = String::new();
    for y in 0..buffer.area().height {
        for x in 0..buffer.area().width {
            s.push(buffer[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        s.push('\n');
    }
    s
}
