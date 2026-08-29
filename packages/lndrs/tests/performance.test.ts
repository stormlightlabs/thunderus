import { expect, test } from "bun:test";
import { measureReplayPerformance } from "../src/replay-benchmark.ts";

test("replay rendering stays within interactive performance budgets", async () => {
  const measurements = await measureReplayPerformance();

  expect(measurements.startup_ms).toBeLessThan(250);
  expect(measurements.idle_rss_mb).toBeLessThan(256);
  expect(measurements.long_replay_ms).toBeLessThan(1_000);
  expect(measurements.long_replay_rss_mb - measurements.idle_rss_mb).toBeLessThan(128);
  expect(measurements.dense_stream_cpu_ms).toBeLessThan(500);
  expect(measurements.input_latency_ms).toBeLessThan(50);
  expect(measurements.completed_history_blocks).toBe(120);
  expect(measurements.streaming_reconciliations).toBe(1);
});
