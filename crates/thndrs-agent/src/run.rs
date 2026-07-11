//! Provider-neutral background-run ownership.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::CancelToken;

/// A running agent turn exposed through a semantic event stream.
///
/// The application supplies the closure that performs provider requests and
/// tool execution. This type owns only the background thread, event channel,
/// and cooperative cancellation handle.
#[derive(Debug)]
pub struct AgentRun<Event> {
    events: Receiver<Event>,
    cancel: CancelToken,
}

impl<Event: Send + 'static> AgentRun<Event> {
    /// Start a provider-neutral run on a background thread.
    pub fn spawn(cancel: CancelToken, run: impl FnOnce(Sender<Event>, CancelToken) + Send + 'static) -> Self {
        let thread_cancel = cancel.clone();
        let (sender, events) = mpsc::channel();
        thread::spawn(move || run(sender, thread_cancel));
        Self { events, cancel }
    }

    /// Consume the run and return its event receiver.
    pub fn into_events(self) -> Receiver<Event> {
        self.events
    }

    /// Return the cooperative cancellation handle for this run.
    pub fn cancel(&self) -> &CancelToken {
        &self.cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_exposes_events_and_the_shared_cancel_token() {
        let cancel = CancelToken::new();
        let run = AgentRun::spawn(cancel.clone(), |sender, received_cancel| {
            sender.send(received_cancel.is_cancelled()).expect("send event");
        });

        assert!(!run.cancel().is_cancelled());
        assert_eq!(run.into_events().recv().expect("receive event"), false);
    }
}
