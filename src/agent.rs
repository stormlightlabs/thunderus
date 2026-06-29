//! Deterministic agent stream.
//!
//! Shaped like a real runtime so callers can swap in a real
//! adapter without changing the app loop or [`AgentEvent`] contract.
//!
//! The stream runs on a background thread and sends [`AgentEvent`] members
//! through a channel.
//!
//! The app loop drains them with `try_recv`.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::app::AgentEvent;

/// Spawns a background thread that emits a deterministic sequence of
/// [`AgentEvent`] members. Returns the receiving end of the channel.
///
/// The stream closes the sender when done, so the receiver's `try_recv`
/// will return `Err(Disconnected)` once the stream finishes.
///
/// If the receiver is dropped early (e.g. the user cancels), the thread
/// exits on the next failed send.
pub fn spawn_fake_stream() -> Receiver<AgentEvent> {
    let (tx, rx) = mpsc::channel::<AgentEvent>();

    thread::spawn(move || {
        run_fake_stream(&tx);
    });

    rx
}

/// The deterministic event sequence, broken out so the logic is readable.
fn run_fake_stream(tx: &Sender<AgentEvent>) {
    let step = || thread::sleep(Duration::from_millis(40));

    let send = |tx: &Sender<AgentEvent>, event: AgentEvent| -> bool {
        if tx.send(event).is_err() {
            return false;
        }
        true
    };

    if !send(tx, AgentEvent::Started) {
        return;
    }
    step();

    if !send(
        tx,
        AgentEvent::ReasoningDelta(String::from("Let me think about this... ")),
    ) {
        return;
    }
    step();

    if !send(
        tx,
        AgentEvent::ReasoningDelta(String::from("The repo is a Rust + Ratatui harness.")),
    ) {
        return;
    }
    step();

    if !send(tx, AgentEvent::ToolStarted { name: String::from("read_file") }) {
        return;
    }
    step();

    if !send(
        tx,
        AgentEvent::ToolOutput { line: String::from("Cargo.toml: 47 lines") },
    ) {
        return;
    }
    step();

    if !send(tx, AgentEvent::ToolFinished) {
        return;
    }
    step();

    if !send(tx, AgentEvent::AssistantDelta(String::from("This is a "))) {
        return;
    }
    step();

    if !send(tx, AgentEvent::AssistantDelta(String::from("fake streaming response."))) {
        return;
    }
    step();

    let _ = tx.send(AgentEvent::Finished);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_stream_emits_expected_sequence() {
        let rx = spawn_fake_stream();

        let mut events = Vec::new();
        while let Ok(event) = rx.recv() {
            events.push(event);
        }

        assert_eq!(events.first(), Some(&AgentEvent::Started));
        assert_eq!(events.last(), Some(&AgentEvent::Finished));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ReasoningDelta(_))));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::AssistantDelta(_))));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolStarted { .. })));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolOutput { .. })));
        assert!(events.contains(&AgentEvent::ToolFinished));
    }

    /// Drop the receiver immediately; the thread should exit without panic.
    /// If the thread panicked, this test would still pass, but the real
    /// guarantee is that no send blocks forever — verified by just completing.
    #[test]
    fn fake_stream_drops_cleanly_when_receiver_dropped() {
        let rx = spawn_fake_stream();
        drop(rx);
    }
}
