use std::hint::black_box;

use divan::{
    Bencher,
    counter::{BytesCount, ItemsCount},
};
use thndrs_agent::{
    BaselinePolicy, CandidatePolicy, ReplayEvaluator, ReplayFixture, load_fixture, project_fixture, select_items,
};

const FIXTURE: &str = include_str!("../fixtures/context-replay/context.json");

fn fixture() -> ReplayFixture {
    load_fixture(FIXTURE).expect("repository fixture is valid")
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
