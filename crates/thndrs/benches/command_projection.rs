use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

use divan::{
    Bencher,
    counter::{BytesCount, ItemsCount},
};
use thndrs_agent::context::ReductionConfig;
use thndrs_lib::tools::command_projection::project;
use thndrs_lib::tools::shell::{ProcessKind, ProcessResult, ProcessStatus};

fn fixture() -> ProcessResult {
    ProcessResult {
        process_id: None,
        command: vec!["cargo".to_string(), "test".to_string(), "--workspace".to_string()],
        cwd: PathBuf::from("/workspace"),
        status: ProcessStatus::Failed,
        exit_code: Some(101),
        stdout: (0..100).map(|index| format!("progress {index}")).collect(),
        stderr: (0..100)
            .flat_map(|index| {
                [
                    format!("error[E0308]: diagnostic {index}"),
                    format!("  --> crates/thndrs/src/core/module_{index}/mod.rs:{}:9", index + 1),
                    format!("test module_{index}::fails ... FAILED"),
                ]
            })
            .chain(std::iter::once("test result: FAILED. 0 passed; 100 failed".to_string()))
            .collect(),
        output_truncated: true,
        elapsed: Duration::from_millis(1_000),
        kind: ProcessKind::OneShot,
    }
}

fn config() -> ReductionConfig {
    let mut config = ReductionConfig::disabled();
    config.command_result = true;
    config
}

fn main() {
    divan::main();
}

#[divan::bench]
fn command_result_projection(bencher: Bencher) {
    let result = fixture();
    let baseline = result.to_output_lines();
    let projected = project("bench", &baseline, &result, "artifact_v1_bench", &config()).expect("projection");
    bencher
        .counter(BytesCount::from(projected.receipt.after_bytes))
        .counter(ItemsCount::from(projected.lines.len()))
        .bench(|| black_box(project("bench", &baseline, &result, "artifact_v1_bench", &config())));
}
