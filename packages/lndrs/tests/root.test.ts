import { expect, test } from "bun:test";
import { createTestRenderer } from "@opentui/core/testing";
import { flushSync, tick } from "svelte";
import type { FrontendSnapshot, TranscriptItem } from "../src/protocol/messages.ts";
import { AppState } from "../src/state/app.svelte.ts";
import { bindRootView } from "../src/views/projection.svelte.ts";
import { createRootView } from "../src/views/root.ts";

const snapshot = (transcript: TranscriptItem[]): FrontendSnapshot => ({
  event_sequence: 0,
  session: { id: "session-1", ephemeral: true, turn_count: 1 },
  workspace: "/tmp/project",
  model: "fake-agent",
  reasoning_effort: "medium",
  run: { state: "idle" },
  transcript,
  queue: [],
  usage: { input_tokens: 12, output_tokens: 4 },
  truncated: false,
});

test("renders the initial shell and updates retained renderables", async () => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  const view = createRootView(renderer);
  const transcript = view.transcript;
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);

  try {
    await renderOnce();
    expect(captureCharFrame()).toContain("Landorus");
    expect(captureCharFrame()).toContain("Ask Landorus…");

    state.apply({ type: "status.updated", message: "Backend ready" });
    await tick();
    await renderOnce();
    expect(view.transcript).toBe(transcript);
    expect(captureCharFrame()).toContain("Backend ready");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test.each([
  [36, 18],
  [100, 24],
])("renders every transcript block at %ix%i", async (width, height) => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width, height });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize(
    snapshot([
      { kind: "user", id: "user-1", text: "Refactor the prompt renderer." },
      { kind: "assistant", id: "assistant-1", text: "I’ll inspect the state first.", streaming: false },
      { kind: "reasoning", id: "reasoning-1", text: "The prompt and session state are coupled.", streaming: false },
      {
        kind: "tool",
        id: "tool-1",
        name: "read",
        arguments: '{"path":"src/ui/prompt.ts"}',
        status: "ok",
        output: ["const prompt = state.prompt"],
      },
      { kind: "skill", id: "skill-1", name: "tui-design", path: ".agents/skills/tui-design/SKILL.md" },
      { kind: "status", id: "status-1", text: "Retrying in 2s" },
      { kind: "error", id: "error-1", text: "Snapshot update failed" },
    ]),
  );

  try {
    await tick();
    await renderOnce();
    view.transcript.scroll.scrollTo(0);
    await renderOnce();
    const top = captureCharFrame();
    expect(top).toContain("you");
    expect(top).toContain("landorus");
    expect(top).toContain("reasoning");
    expect(top).toContain("read");

    view.transcript.scroll.scrollTo(view.transcript.scroll.scrollHeight);
    await renderOnce();
    const bottom = captureCharFrame();
    expect(bottom).toContain("skill");
    expect(bottom).toContain("Retrying in 2s");
    expect(bottom).toContain("Snapshot update failed");
    expect(view.transcript.blockCount).toBe(7);
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("renders compact context status and a temporary inspector", async () => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 70, height: 20 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize({
    ...snapshot([]),
    context: {
      used_tokens: 74_000,
      context_window: 128_000,
      available_input: 110_000,
      target_tokens: 88_000,
      auto_compaction_threshold: 101_200,
      compaction_state: "idle",
      limit_source: "static_provider",
    },
  });

  try {
    await tick();
    await renderOnce();
    expect(captureCharFrame()).toContain("74k / 128k · 58%");

    state.openOverlay("context");
    await tick();
    await renderOnce();
    const frame = captureCharFrame();
    expect(frame).toContain("CONTEXT");
    expect(frame).toContain("compact at");
    expect(frame).toContain("Esc  return to Stream");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("renders permission and provider-supported reasoning pickers", async () => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 70, height: 20 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize({
    ...snapshot([]),
    capabilities: {
      commands: ["permission.respond", "model.select", "reasoning.select"],
      models: [{ label: "fake-agent", detail: "Fake model" }],
      reasoning_efforts: [
        { value: "auto", label: "Auto", description: "Provider default" },
        { value: "high", label: "High", description: "Difficult work" },
      ],
    },
  });

  try {
    state.openOverlay("reasoning");
    await tick();
    await renderOnce();
    expect(captureCharFrame()).toContain("REASONING EFFORT");
    expect(captureCharFrame()).toContain("Difficult work");

    state.apply({
      type: "permission.requested",
      tool_call_id: "call-1",
      title: "Run command?",
      selected: 0,
      options: [
        { id: "allow", name: "Allow once", kind: "allow once" },
        { id: "reject", name: "Always reject", kind: "reject always" },
      ],
    });
    await tick();
    await renderOnce();
    const frame = captureCharFrame();
    expect(frame).toContain("PERMISSION REQUIRED");
    expect(frame).toContain("Allow once");
    expect(frame).toContain("Always reject");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("command palette is searchable and capability sourced", async () => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 70, height: 18 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize({
    ...snapshot([]),
    context: {
      used_tokens: 10,
      context_window: 100,
      available_input: 80,
      target_tokens: 64,
      auto_compaction_threshold: 74,
      compaction_state: "idle",
      limit_source: "fallback",
    },
    capabilities: {
      commands: ["model.select"],
      models: [{ label: "fake-agent", detail: "Fake" }],
      reasoning_efforts: [],
    },
  });

  try {
    state.openOverlay("palette");
    await tick();
    state.setOverlayQuery("model");
    await tick();
    await renderOnce();
    const frame = captureCharFrame();
    expect(frame).toContain("Model: select model");
    expect(frame).not.toContain("Context: inspect usage");
    expect(frame).not.toContain("Reasoning: select effort");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("updates live and tool blocks without replacing their renderables", async () => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 70, height: 16 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize(snapshot([]));

  try {
    state.apply({ type: "assistant.delta", text: "Hello" });
    state.apply({ type: "tool.started", id: "call-1", name: "bash", arguments: '{"command":"cargo test"}' });
    await tick();
    const assistant = view.transcript.getBlock("live-1");
    const tool = view.transcript.getBlock("call-1");

    state.apply({ type: "tool.finished", id: "call-1", status: "ok", output: ["8 passed"] });
    state.apply({ type: "assistant.delta", text: "Done" });
    await tick();
    await renderOnce();

    expect(view.transcript.getBlock("live-1")).toBe(assistant);
    expect(view.transcript.getBlock("call-1")).toBe(tool);
    expect(view.transcript.blockCount).toBe(3);
    expect(captureCharFrame()).toContain("✓ bash  cargo test");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("distinguishes every tool lifecycle state", async () => {
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 70, height: 18 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize(
    snapshot(
      ["running", "ok", "failed", "cancelled"].map((status) => ({
        kind: "tool" as const,
        id: `tool-${status}`,
        name: status,
        arguments: "{}",
        status,
        output: [],
      })),
    ),
  );

  try {
    await tick();
    await renderOnce();
    const frame = captureCharFrame();
    expect(frame).toContain("◐ running");
    expect(frame).toContain("✓ ok");
    expect(frame).toContain("✕ failed");
    expect(frame).toContain("■ cancelled");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("coalesces dense streaming projection work without dropping state events", async () => {
  const { renderer } = await createTestRenderer({ width: 70, height: 14 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);

  const dispose = bindRootView(state, view);
  state.initialize(snapshot([]));
  flushSync();

  try {
    const beforeFlushPerEvent = view.transcript.reconciliationCount;
    for (let index = 0; index < 100; index += 1) {
      state.apply({ type: "assistant.delta", text: "a" });
      await tick();
    }
    const flushPerEventProjectionCount = view.transcript.reconciliationCount - beforeFlushPerEvent;

    const beforeBatched = view.transcript.reconciliationCount;
    for (let index = 0; index < 100; index += 1) {
      state.apply({ type: "assistant.delta", text: "b" });
    }
    await tick();
    const batchedProjectionCount = view.transcript.reconciliationCount - beforeBatched;

    expect([flushPerEventProjectionCount, batchedProjectionCount]).toEqual([100, 1]);
    expect(state.transcript[0]).toMatchObject({ text: `${"a".repeat(100)}${"b".repeat(100)}` });
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("does not add renderables once per streaming token", async () => {
  const { renderer } = await createTestRenderer({ width: 70, height: 14 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize(snapshot([]));

  try {
    state.apply({ type: "assistant.delta", text: "a" });
    await tick();
    const block = view.transcript.getBlock("live-1");
    for (let index = 0; index < 1_000; index += 1) {
      state.apply({ type: "assistant.delta", text: "b" });
    }
    await tick();

    expect(view.transcript.blockCount).toBe(1);
    expect(view.transcript.getBlock("live-1")).toBe(block);
    expect(state.transcript[0]).toMatchObject({ text: `a${"b".repeat(1_000)}` });
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("expands bounded tool details in place", async () => {
  const { renderer, renderOnce, captureCharFrame, mockMouse } = await createTestRenderer({
    width: 70,
    height: 14,
    useMouse: true,
  });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  state.initialize(
    snapshot([
      {
        kind: "tool",
        id: "call-1",
        name: "read",
        arguments: '{"path":"src/ui/prompt.ts"}',
        status: "failed",
        output: ["file not found"],
      },
    ]),
  );

  try {
    await tick();
    await renderOnce();
    expect(captureCharFrame()).not.toContain("file not found");
    const block = view.transcript.getBlock("call-1");
    expect(block).toBeDefined();
    await mockMouse.click(block!.root.screenX + 1, block!.root.screenY + 1);
    await renderOnce();
    expect(view.transcript.getBlock("call-1")).toBe(block);
    expect(captureCharFrame()).toContain("arguments");
    expect(captureCharFrame()).toContain("file not found");
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("new deltas do not pull historical scrolling back to the bottom", async () => {
  const { renderer, renderOnce } = await createTestRenderer({ width: 50, height: 12 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  const history: TranscriptItem[] = Array.from({ length: 20 }, (_, index) => ({
    kind: "assistant" as const,
    id: `assistant-${index}`,
    text: `Historical response ${index}\nwith another line`,
    streaming: index === 19,
  }));
  state.initialize(snapshot(history));

  try {
    await tick();
    await renderOnce();
    expect(view.transcript.scroll.scrollTop).toBeGreaterThan(0);
    view.transcript.scroll.scrollTo(2);
    const historicalPosition = view.transcript.scroll.scrollTop;

    state.apply({ type: "status.updated", message: "Still working" });
    state.apply({ type: "usage.updated", input_tokens: 20, output_tokens: 5 });
    await tick();
    await renderOnce();
    expect(view.transcript.scroll.scrollTop).toBe(historicalPosition);

    state.apply({ type: "assistant.delta", text: "\nnew output" });
    await tick();
    await renderOnce();
    expect(view.transcript.scroll.scrollTop).toBe(historicalPosition);

    const bottom = view.transcript.scroll.scrollHeight - view.transcript.scroll.viewport.height;
    view.transcript.scroll.scrollTo(bottom);
    state.apply({ type: "assistant.delta", text: "\nmore output" });
    await tick();
    await renderOnce();
    expect(view.transcript.scroll.scrollTop).toBe(
      view.transcript.scroll.scrollHeight - view.transcript.scroll.viewport.height,
    );
  } finally {
    dispose();
    renderer.destroy();
  }
});
