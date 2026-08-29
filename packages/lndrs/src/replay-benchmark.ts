import { createTestRenderer } from "@opentui/core/testing";
import { performance } from "node:perf_hooks";
import { resolve } from "node:path";
import { tick } from "svelte";
import { loadReplayFixture, playReplay } from "./replay.ts";
import { AppState } from "./state/app.svelte.ts";
import { bindRootView } from "./views/projection.svelte.ts";
import { createRootView } from "./views/root.ts";

const fixtureDirectory = resolve(import.meta.dir, "../../../crates/thndrs/tests/fixtures/frontend-replay");

export type ReplayMeasurements = {
  schema_version: "lndrs-replay-measurements-v1";
  startup_ms: number;
  idle_rss_mb: number;
  long_replay_ms: number;
  long_replay_rss_mb: number;
  dense_stream_cpu_ms: number;
  input_latency_ms: number;
  completed_history_blocks: number;
  streaming_reconciliations: number;
};

export async function measureReplayPerformance(): Promise<ReplayMeasurements> {
  const startupStarted = performance.now();
  const testRenderer = await createTestRenderer({ width: 100, height: 30 });
  const state = new AppState();
  const view = createRootView(testRenderer.renderer);
  testRenderer.renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  const simple = await loadReplayFixture(`${fixtureDirectory}/simple-turn.json`);
  state.initialize(simple.snapshot);
  await tick();
  await testRenderer.renderOnce();
  const startupMs = performance.now() - startupStarted;
  const idleRssMb = bytesToMegabytes(process.memoryUsage().rss);

  try {
    const long = await loadReplayFixture(`${fixtureDirectory}/long-transcript.json`);
    const longStarted = performance.now();
    state.replaceSnapshot(long.snapshot);
    await playReplay(long, (event) => state.apply(event));
    await tick();
    await testRenderer.renderOnce();
    const longReplayMs = performance.now() - longStarted;
    const longReplayRssMb = bytesToMegabytes(process.memoryUsage().rss);

    const reconciliationsBefore = view.transcript.reconciliationCount;
    const cpuStarted = process.cpuUsage();
    for (let index = 0; index < 2_000; index += 1) state.apply({ type: "assistant.delta", text: "x" });
    view.composer.input.focus();
    const inputStarted = performance.now();
    await testRenderer.mockInput.typeText("responsive");
    const inputLatencyMs = performance.now() - inputStarted;
    await tick();
    await testRenderer.renderOnce();
    const cpu = process.cpuUsage(cpuStarted);
    const denseStreamCpuMs = (cpu.user + cpu.system) / 1_000;

    return {
      schema_version: "lndrs-replay-measurements-v1",
      startup_ms: round(startupMs),
      idle_rss_mb: round(idleRssMb),
      long_replay_ms: round(longReplayMs),
      long_replay_rss_mb: round(longReplayRssMb),
      dense_stream_cpu_ms: round(denseStreamCpuMs),
      input_latency_ms: round(inputLatencyMs),
      completed_history_blocks: long.snapshot.transcript.length,
      streaming_reconciliations: view.transcript.reconciliationCount - reconciliationsBefore,
    };
  } finally {
    dispose();
    testRenderer.renderer.destroy();
  }
}

function bytesToMegabytes(bytes: number): number {
  return bytes / (1024 * 1024);
}

function round(value: number): number {
  return Math.round(value * 100) / 100;
}

if (import.meta.main) console.log(JSON.stringify(await measureReplayPerformance(), null, 2));
