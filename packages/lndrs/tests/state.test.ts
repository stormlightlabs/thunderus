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

test("keeps one active block per streaming semantic kind", () => {
  const state = new AppState();
  state.initialize(structuredClone(snapshot));
  state.apply({ type: "reasoning.delta", text: "Think" });
  state.apply({ type: "assistant.delta", text: "Answer" });
  state.apply({ type: "reasoning.delta", text: " again" });
  state.apply({ type: "assistant.delta", text: " now" });

  expect(state.transcript).toHaveLength(2);
  expect(state.transcript[0]).toMatchObject({ kind: "reasoning", id: "live-1", text: "Think again" });
  expect(state.transcript[1]).toMatchObject({ kind: "assistant", id: "live-2", text: "Answer now" });

  state.apply({ type: "tool.started", id: "call-1", name: "read", arguments: "{}" });
  state.apply({ type: "assistant.delta", text: "After tool" });
  expect(state.transcript).toHaveLength(4);
  expect(state.transcript[0]).toMatchObject({ streaming: false });
  expect(state.transcript[1]).toMatchObject({ streaming: false });
});

test("updates a tool by protocol call ID instead of appending lifecycle entries", () => {
  const state = new AppState();
  state.initialize(structuredClone(snapshot));
  state.apply({ type: "tool.started", id: "call-1", name: "bash", arguments: '{"command":"cargo test"}' });
  state.apply({ type: "tool.finished", id: "call-1", status: "failed", output: ["test failed"] });

  expect(state.transcript).toHaveLength(1);
  expect(state.transcript[0]).toEqual({
    kind: "tool",
    id: "call-1",
    name: "bash",
    arguments: '{"command":"cargo test"}',
    status: "failed",
    output: ["test failed"],
  });
});
