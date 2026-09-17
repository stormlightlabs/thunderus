//! Write the deterministic capture fixtures into a scratch session directory.
//!
//! The terminal capture harness runs this before a capture pass and points
//! `thndrs --session-dir` at the directory it wrote:
//!
//! ```sh
//! cargo run -p thndrs --features dev-fixtures --example session_fixtures
//! ```
//!
//! An example behind the `dev-fixtures` feature rather than a subcommand: the
//! capture set is comparable only while no user-visible surface moves to
//! accommodate it, and a released build has no reason to carry a generator.
//! The optional directory argument resolves against the shell; with none, the
//! output goes to [`thndrs_lib::session::fixtures::default_fixture_dir`], under
//! the workspace root's ignored `target/`.

use std::path::PathBuf;

use thndrs_lib::session::fixtures;

fn main() -> std::io::Result<()> {
    let dir = match std::env::args().nth(1) {
        Some(argument) => PathBuf::from(argument),
        None => fixtures::default_fixture_dir()?,
    };
    let generated = fixtures::generate(&dir)?;

    println!("{} fixture sessions in {}", generated.len(), dir.display());
    for fixture in &generated {
        println!("  {:<24} {} records", fixture.session_id, fixture.record_count);
    }
    Ok(())
}
