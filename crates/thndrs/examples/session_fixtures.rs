//! Write the deterministic capture fixtures into a scratch session directory.
//!
//! The terminal capture harness runs this before a capture pass and points
//! `thndrs --session-dir` at the directory it wrote:
//!
//! ```sh
//! cargo run --example session_fixtures -- target/tui-fixtures/sessions
//! ```
//!
//! An example rather than a subcommand: fixtures are a development concern and
//! the capture set is only comparable while no user-visible surface changes to
//! accommodate it. The directory argument is optional and defaults to
//! [`thndrs_lib::session::fixtures::default_fixture_dir`], which `.gitignore`
//! already covers.

use std::path::PathBuf;

use thndrs_lib::session::fixtures;

fn main() -> std::io::Result<()> {
    let dir = std::env::args()
        .nth(1)
        .map_or_else(fixtures::default_fixture_dir, PathBuf::from);
    let generated = fixtures::generate(&dir)?;

    println!("{} fixture sessions in {}", generated.len(), dir.display());
    for fixture in &generated {
        println!("  {:<24} {} records", fixture.session_id, fixture.record_count);
    }
    Ok(())
}
