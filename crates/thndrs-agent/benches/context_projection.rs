use std::hint::black_box;

use divan::{
    Bencher,
    counter::{BytesCount, ItemsCount},
};
use thndrs_agent::context::{ReducerKind, ReductionConfig};
use thndrs_agent::{
    BaselinePolicy, CandidatePolicy, ReplayEvaluator, ReplayFixture, load_fixture, project_fixture, reduce_lines,
    select_items,
};

const FIXTURE: &str = include_str!("../fixtures/context-replay/context.json");

fn fixture() -> ReplayFixture {
    load_fixture(FIXTURE).expect("repository fixture is valid")
}

fn reduction_fixture() -> Vec<String> {
    vec![
        "\u{1b}[32mcompile\u{1b}[0m 10%\r\u{1b}[32mcompile\u{1b}[0m 100%".to_string(),
        "".to_string(),
        "".to_string(),
        "same line".to_string(),
        "same line".to_string(),
        "same line".to_string(),
        "same line".to_string(),
        "same line".to_string(),
        "after".to_string(),
    ]
}

fn reduction_config(kind: ReducerKind) -> ReductionConfig {
    let mut config = ReductionConfig::disabled();
    match kind {
        ReducerKind::TerminalControl => config.terminal_control = true,
        ReducerKind::ProgressRedraw => config.progress_redraw = true,
        ReducerKind::BlankRun => config.blank_run = true,
        ReducerKind::RepeatedLine => config.repeated_line = true,
    }
    config
}

fn main() {
    divan::main();
}

#[divan::bench]
fn selection(bencher: Bencher) {
    let fixture = fixture();
    let item_count = select_items(&fixture, &BaselinePolicy).len();
    bencher
        .counter(ItemsCount::from(item_count))
        .bench(|| black_box(select_items(&fixture, &BaselinePolicy)));
}

#[divan::bench]
fn projection(bencher: Bencher) {
    let fixture = fixture();
    let projection = project_fixture(&fixture, &BaselinePolicy);
    bencher
        .counter(BytesCount::from(projection.exact_bytes.value))
        .counter(ItemsCount::from(projection.item_ids.len()))
        .bench(|| black_box(project_fixture(&fixture, &BaselinePolicy)));
}

#[divan::bench]
fn candidate_projection(bencher: Bencher) {
    let fixture = fixture();
    let candidate = CandidatePolicy::new("candidate").omit("progress-1").omit("progress-2");
    let projection = project_fixture(&fixture, &candidate);
    bencher
        .counter(BytesCount::from(projection.exact_bytes.value))
        .counter(ItemsCount::from(projection.item_ids.len()))
        .bench(|| black_box(project_fixture(&fixture, &candidate)));
}

#[divan::bench]
fn receipt_generation(bencher: Bencher) {
    let fixture = fixture();
    let receipt_count = fixture.items.len();
    bencher
        .counter(ItemsCount::from(receipt_count))
        .bench(|| black_box(project_fixture(&fixture, &BaselinePolicy).receipts));
}

#[divan::bench]
fn export_and_evaluation(bencher: Bencher) {
    let fixture = fixture();
    let report = ReplayEvaluator::new()
        .evaluate(&fixture, &BaselinePolicy, &CandidatePolicy::new("candidate"))
        .expect("baseline candidate preserve the fixture");
    let output_bytes = report.to_json().expect("json report").len() + report.to_markdown().len();
    bencher
        .counter(BytesCount::from(output_bytes))
        .counter(ItemsCount::from(
            report.baseline.item_count + report.candidate.item_count,
        ))
        .bench(|| {
            let report = ReplayEvaluator::new()
                .evaluate(&fixture, &BaselinePolicy, &CandidatePolicy::new("candidate"))
                .expect("baseline candidate preserve the fixture");
            black_box(report.to_json().expect("json report").len() + report.to_markdown().len())
        });
}

#[divan::bench]
fn reducer_terminal_control(bencher: Bencher) {
    let input = reduction_fixture();
    let config = reduction_config(ReducerKind::TerminalControl);
    let result = reduce_lines("bench", input.clone(), &config);
    bencher
        .counter(BytesCount::from(result.dashboard.after_bytes))
        .counter(ItemsCount::from(result.lines.len()))
        .bench(|| black_box(reduce_lines("bench", input.clone(), &config)));
}

#[divan::bench]
fn reducer_progress_redraw(bencher: Bencher) {
    let input = reduction_fixture();
    let config = reduction_config(ReducerKind::ProgressRedraw);
    let result = reduce_lines("bench", input.clone(), &config);
    bencher
        .counter(BytesCount::from(result.dashboard.after_bytes))
        .counter(ItemsCount::from(result.lines.len()))
        .bench(|| black_box(reduce_lines("bench", input.clone(), &config)));
}

#[divan::bench]
fn reducer_blank_run(bencher: Bencher) {
    let input = reduction_fixture();
    let config = reduction_config(ReducerKind::BlankRun);
    let result = reduce_lines("bench", input.clone(), &config);
    bencher
        .counter(BytesCount::from(result.dashboard.after_bytes))
        .counter(ItemsCount::from(result.lines.len()))
        .bench(|| black_box(reduce_lines("bench", input.clone(), &config)));
}

#[divan::bench]
fn reducer_repeated_line(bencher: Bencher) {
    let input = reduction_fixture();
    let config = reduction_config(ReducerKind::RepeatedLine);
    let result = reduce_lines("bench", input.clone(), &config);
    bencher
        .counter(BytesCount::from(result.dashboard.after_bytes))
        .counter(ItemsCount::from(result.lines.len()))
        .bench(|| black_box(reduce_lines("bench", input.clone(), &config)));
}
