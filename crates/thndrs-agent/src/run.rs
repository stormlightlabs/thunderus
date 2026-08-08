//! Provider-neutral background-run ownership.

use std::sync::mpsc::{self, Iter, Receiver, RecvError, RecvTimeoutError, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use thiserror::Error;

use crate::CancelToken;

/// Failure observed while settling an agent worker.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AgentRunError {
    /// The worker unwound before it could settle normally.
    #[error("agent worker panicked")]
    WorkerPanicked,
}

/// A running agent turn exposed through a semantic event stream.
///
/// The application supplies the closure that performs provider requests and
/// tool execution. This type owns the background thread, event channel, and
/// cooperative cancellation handle.
///
/// Dropping a run requests cancellation, disconnects its event receiver, and
/// joins the worker. Workers must still cooperate with cancellation or stop
/// after event delivery fails; an arbitrary blocked worker cannot be forced to
/// exit safely by the standard library.
#[derive(Debug)]
pub struct AgentRun<Event> {
    events: Receiver<Event>,
    cancel: CancelToken,
    worker: Option<JoinHandle<()>>,
}

impl<Event: Send + 'static> AgentRun<Event> {
    /// Start a provider-neutral run on a background thread.
    pub fn spawn(cancel: CancelToken, run: impl FnOnce(Sender<Event>, CancelToken) + Send + 'static) -> Self {
        let thread_cancel = cancel.clone();
        let (sender, events) = mpsc::channel();
        let worker = thread::spawn(move || run(sender, thread_cancel));
        Self { events, cancel, worker: Some(worker) }
    }
}

impl<Event> AgentRun<Event> {
    /// Receive the next event, blocking until the worker sends or disconnects.
    pub fn recv(&self) -> Result<Event, RecvError> {
        self.events().recv()
    }

    /// Try to receive the next event without blocking.
    pub fn try_recv(&self) -> Result<Event, TryRecvError> {
        self.events().try_recv()
    }

    /// Receive the next event until the timeout elapses.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Event, RecvTimeoutError> {
        self.events().recv_timeout(timeout)
    }

    /// Iterate over events until the worker disconnects.
    pub fn iter(&self) -> Iter<'_, Event> {
        self.events().iter()
    }

    /// Return the cooperative cancellation handle for this run.
    pub fn cancel(&self) -> &CancelToken {
        &self.cancel
    }

    /// Join the worker and report whether it panicked.
    ///
    /// Call this after observing a terminal event or receiver disconnection.
    /// Calling it while a worker still needs the consumer to drain events can
    /// block.
    pub fn wait(&mut self) -> Result<(), AgentRunError> {
        self.join_worker()
    }

    /// Request cancellation, disconnect event delivery, and join the worker.
    pub fn cancel_and_wait(&mut self) -> Result<(), AgentRunError> {
        self.cancel.cancel();
        self.disconnect_events();
        self.join_worker()
    }

    /// Request cancellation and relinquish the worker without blocking the caller.
    ///
    /// This is reserved for a bounded UI shutdown path after the worker has not
    /// settled within its cancellation grace period. The worker must still
    /// honor cancellation or its own operation deadlines; dropping the join
    /// handle only prevents an uncooperative worker from freezing its caller.
    pub fn detach(mut self) {
        self.cancel.cancel();
        self.disconnect_events();
        drop(self.worker.take());
    }

    fn events(&self) -> &Receiver<Event> {
        &self.events
    }

    fn disconnect_events(&mut self) {
        let (_sender, disconnected) = mpsc::channel();
        self.events = disconnected;
    }

    fn join_worker(&mut self) -> Result<(), AgentRunError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        worker.join().map_err(|_| AgentRunError::WorkerPanicked)
    }
}

impl<Event> Drop for AgentRun<Event> {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.disconnect_events();
        let _ = self.join_worker();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn run_exposes_events_cancel_and_completion() {
        let cancel = CancelToken::new();
        let mut run = AgentRun::spawn(cancel, |sender, received_cancel| {
            sender.send(received_cancel.is_cancelled()).expect("send event");
        });

        assert!(!run.cancel().is_cancelled());
        assert!(!run.recv().expect("receive event"));
        assert_eq!(run.wait(), Ok(()));
    }

    #[test]
    fn wait_reports_worker_panic() {
        let mut run = AgentRun::<()>::spawn(CancelToken::new(), |_sender, _cancel| {
            panic!("worker failed");
        });

        assert_eq!(run.recv(), Err(RecvError));
        assert_eq!(run.wait(), Err(AgentRunError::WorkerPanicked));
    }

    #[test]
    fn detach_does_not_wait_for_an_uncooperative_worker() {
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (settled_tx, settled_rx) = std::sync::mpsc::channel();
        let run = AgentRun::<()>::spawn(CancelToken::new(), move |_sender, _cancel| {
            release_rx.recv().expect("release worker");
            settled_tx.send(()).expect("signal worker completion");
        });

        run.detach();
        release_tx.send(()).expect("release detached worker");
        settled_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("detached worker should finish");
    }

    #[test]
    fn drop_requests_cancellation_and_joins() {
        let settled = Arc::new(AtomicBool::new(false));
        let worker_settled = settled.clone();
        let run = AgentRun::<()>::spawn(CancelToken::new(), move |_sender, cancel| {
            while !cancel.is_cancelled() {
                thread::yield_now();
            }
            worker_settled.store(true, Ordering::SeqCst);
        });

        drop(run);

        assert!(settled.load(Ordering::SeqCst));
    }

    #[test]
    fn drop_disconnects_event_delivery_before_joining() {
        let settled = Arc::new(AtomicBool::new(false));
        let worker_settled = settled.clone();
        let run = AgentRun::spawn(CancelToken::new(), move |sender, _cancel| {
            while sender.send(()).is_ok() {
                thread::yield_now();
            }
            worker_settled.store(true, Ordering::SeqCst);
        });

        drop(run);

        assert!(settled.load(Ordering::SeqCst));
    }
}
