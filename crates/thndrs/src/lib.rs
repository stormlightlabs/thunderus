//! `thndrs` library entrypoint.
//!
//! Terminal setup, command routing, and the interactive loop live in
//! [`runtime`]; this module keeps the public library surface small.

pub mod cli;
pub mod server;

#[path = "core/agent.rs"]
pub mod agent;
#[path = "core/session/mod.rs"]
pub mod session;

mod cancellation;
mod headless;
#[path = "core/mod.rs"]
mod thndrs_core;

pub use cli::{app, input, renderer};

pub use prelude::*;
pub use thndrs_core::{
    acp, artifacts, config, context, fuzzy, harness, internals, mcp, prelude, prompt, providers, review, sandbox,
    search, skills, tools, trust, utils,
};

#[cfg(test)]
pub mod test_env {
    use std::cell::Cell;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    thread_local! {
        static LOCK_DEPTH: Cell<usize> = const { Cell::new(0) };
    }

    pub struct Guard {
        _lock: Option<MutexGuard<'static, ()>>,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            LOCK_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }

    pub fn lock() -> Guard {
        let nested = LOCK_DEPTH.with(|depth| {
            let nested = depth.get() > 0;
            depth.set(depth.get() + 1);
            nested
        });
        let lock = (!nested).then(|| ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
        Guard { _lock: lock }
    }
}

mod runtime;

pub(crate) use runtime::maybe_spawn_agent;
use thndrs_core::utils::datetime;

/// Process exit code for a [`run`] error.
pub fn exit_code(error: &std::io::Error) -> i32 {
    runtime::exit_code(error)
}

/// Run the TUI or one of the non-interactive commands.
pub fn run(cli: &cli::Cli) -> std::io::Result<()> {
    runtime::run(cli)
}

/// Render the `--print-prompt` debug view as a string.
pub fn render_print_prompt(bundle: &prompt::PromptBundle) -> String {
    runtime::render_print_prompt(bundle)
}
