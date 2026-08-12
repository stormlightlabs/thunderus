//! Process-wide cooperative Ctrl-C cancellation for command-line operations.

use std::io;
use std::sync::{Mutex, OnceLock};

use thndrs_agent::CancelToken;

static CANCELLATION: OnceLock<Mutex<Option<CancelToken>>> = OnceLock::new();
static CTRL_C_HANDLER: OnceLock<std::result::Result<(), String>> = OnceLock::new();

/// Clears the active cancellation target when a command finishes.
pub struct CancellationRegistration;

impl Drop for CancellationRegistration {
    fn drop(&mut self) {
        if let Some(slot) = CANCELLATION.get() {
            *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
    }
}

/// Route Ctrl-C to the supplied cooperative cancellation token.
pub fn register(cancellation: CancelToken) -> io::Result<CancellationRegistration> {
    let slot = CANCELLATION.get_or_init(|| Mutex::new(None));
    let registration = CTRL_C_HANDLER.get_or_init(|| {
        ctrlc::set_handler(|| {
            let Some(slot) = CANCELLATION.get() else {
                return;
            };
            let cancellation = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            if let Some(cancellation) = cancellation {
                cancellation.cancel();
            }
        })
        .map_err(|error| error.to_string())
    });
    if let Err(error) = registration {
        return Err(io::Error::other(format!("failed to register Ctrl-C handler: {error}")));
    }
    *slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(cancellation);
    Ok(CancellationRegistration)
}

/// Return an interrupted I/O error when cancellation has been requested.
pub fn check(cancellation: &CancelToken) -> io::Result<()> {
    if cancellation.is_cancelled() {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "session operation cancelled",
        ))
    } else {
        Ok(())
    }
}
