import { expect, test } from "bun:test";
import type { FrontendSnapshot } from "../src/protocol/messages.ts";
import { AppState } from "../src/state/app.svelte.ts";

const snapshot: FrontendSnapshot = {
  event_sequence: 0,
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
  expect(state.statusText).toContain("fake-agent · medium");
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

test("capability gates controls separately from provider options", () => {
  const state = new AppState();
  state.initialize({
    ...structuredClone(snapshot),
    capabilities: {
      commands: ["model.select", "reasoning.select"],
      models: [{ label: "fake-agent", detail: "Fake" }],
      reasoning_efforts: [{ value: "auto", label: "Auto", description: "Provider default" }],
    },
  });

  expect(state.openOverlay("model")).toBe(true);
  state.closeOverlay();
  expect(state.openOverlay("reasoning")).toBe(false);

  state.snapshot!.capabilities!.reasoning_efforts.push({ value: "high", label: "High", description: "Difficult work" });
  expect(state.openOverlay("reasoning")).toBe(true);
});

test("permission requests own focus until backend settlement", () => {
  const state = new AppState();
  state.initialize(structuredClone(snapshot));
  state.apply({
    type: "permission.requested",
    tool_call_id: "call-1",
    title: "Run command?",
    selected: 0,
    options: [{ id: "allow", name: "Allow once", kind: "allow once" }],
  });

  expect(state.overlay?.kind).toBe("permission");
  state.closeOverlay();
  expect(state.overlay?.kind).toBe("permission");
  state.apply({ type: "permission.resolved", tool_call_id: "call-1", outcome: "selected allow" });
  expect(state.pendingPermission).toBeUndefined();
  expect(state.overlay).toBeUndefined();
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
