//! Client-side Agent Client Protocol integration

pub mod config;
pub mod events;
pub mod fs;
pub mod permissions;
pub mod registry;
pub mod runner;
pub mod terminal;

pub use runner::spawn_run;

#[cfg(test)]
mod tests;
