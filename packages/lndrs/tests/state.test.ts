import { expect, test } from "bun:test";
import type { FrontendSnapshot } from "../src/protocol/messages.ts";
import { AppState } from "../src/state/app.svelte.ts";

const snapshot: FrontendSnapshot = {
  session: { id: "session-1", ephemeral: true, turn_count: 0 },
  workspace: "/tmp/project",
  model: "fake-agent",
  reasoning_effort: "medium",
  run: { state: "idle" },
  transcript: [],
  queue: [],
  usage: { input_tokens: 0, output_tokens: 0 },
  truncated: false,
};

test("initializes from a snapshot and applies incremental events", () => {
  const state = new AppState();
  state.initialize(structuredClone(snapshot));
  state.apply({ type: "run.started" });
  state.apply({ type: "assistant.delta", text: "Hello" });
  state.apply({ type: "assistant.delta", text: " world" });
  state.apply({ type: "usage.updated", input_tokens: 12, output_tokens: 4 });

  expect(state.run.state).toBe("working");
  expect(state.transcript).toHaveLength(1);
  expect(state.transcript[0]).toMatchObject({ kind: "assistant", text: "Hello world" });
  expect(state.statusText).toContain("16 tokens");
});
