import { expect, test } from "bun:test";
import { readdir } from "node:fs/promises";
import { resolve } from "node:path";
import { createTestRenderer } from "@opentui/core/testing";
import { tick } from "svelte";
import { loadReplayFixture, parseReplayFixture, playReplay, REPLAY_SCHEMA_VERSION } from "../src/replay.ts";
import { AppState } from "../src/state/app.svelte.ts";
import { bindRootView } from "../src/views/projection.svelte.ts";
import { createRootView } from "../src/views/root.ts";

const fixtureDirectory = resolve(import.meta.dir, "../../../crates/thndrs/tests/fixtures/frontend-replay");
const expectedFixtures = [
  "cancellation",
  "compaction",
  "failed-tool",
  "long-transcript",
  "permission",
  "provider-failure",
  "queued-input",
  "reasoning",
  "retry",
  "simple-turn",
  "streaming",
  "tool-heavy",
];

test("the versioned replay corpus covers every visual QA scenario", async () => {
  const names = (await readdir(fixtureDirectory))
    .filter((name) => name.endsWith(".json"))
    .map((name) => name.replace(/\.json$/, ""))
    .sort();
  expect(names).toEqual(expectedFixtures);

  for (const name of names) {
    const fixture = await loadReplayFixture(`${fixtureDirectory}/${name}.json`);
    expect(fixture.schema_version).toBe(REPLAY_SCHEMA_VERSION);
    expect(fixture.name).toBe(name);
  }
});

test("rejects unversioned and malformed replay fixtures", () => {
  expect(() => parseReplayFixture({})).toThrow(REPLAY_SCHEMA_VERSION);
  expect(() =>
    parseReplayFixture({
      schema_version: REPLAY_SCHEMA_VERSION,
      name: "bad",
      terminal: { width: 0, height: 24 },
      snapshot: {},
      steps: [],
    }),
  ).toThrow("terminal dimensions");
});

test("immediate playback applies every event without sleeping", async () => {
  const fixture = await loadReplayFixture(`${fixtureDirectory}/streaming.json`);
  const state = new AppState();
  state.initialize(fixture.snapshot);
  const delays: number[] = [];

  await playReplay(
    fixture,
    (event) => state.apply(event),
    "immediate",
    async (delay) => {
      delays.push(delay);
    },
  );

  expect(delays).toEqual([]);
  expect(state.run.state).toBe("idle");
  expect(state.transcript.at(-1)).toMatchObject({
    kind: "assistant",
    text: "I’ll inspect the renderer, then verify it.",
  });
});

test("timed playback preserves fixture delays", async () => {
  const fixture = await loadReplayFixture(`${fixtureDirectory}/streaming.json`);
  const delays: number[] = [];
  const events: string[] = [];

  await playReplay(
    fixture,
    (event) => events.push(event.type),
    "timed",
    async (delay) => {
      delays.push(delay);
    },
  );

  expect(delays).toEqual(fixture.steps.map((step) => step.delay_ms).filter((delay) => delay > 0));
  expect(events).toEqual(fixture.steps.map((step) => step.event.type));
});

test.each([
  ["permission", 42, 16],
  ["tool-heavy", 80, 24],
  ["long-transcript", 120, 30],
] as const)("captures the %s replay at %ix%i", async (name, width, height) => {
  const fixture = await loadReplayFixture(`${fixtureDirectory}/${name}.json`);
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width, height });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);

  try {
    state.initialize(fixture.snapshot);
    await playReplay(fixture, (event) => state.apply(event));
    await tick();
    await renderOnce();
    expect(captureCharFrame()).toMatchSnapshot();
    if (name === "long-transcript") {
      expect(view.transcript.scroll.viewportCulling).toBe(true);
      expect(view.transcript.blockCount).toBe(121);
    }
  } finally {
    dispose();
    renderer.destroy();
  }
});
