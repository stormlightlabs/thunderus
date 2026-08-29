import { expect, test } from "bun:test";
import { resolve } from "node:path";
import { createTestRenderer } from "@opentui/core/testing";
import { InteractionController } from "../src/interaction.ts";
import { FrontendClient } from "../src/protocol/client.ts";
import type { Command, FrontendSnapshot, ResponseResult } from "../src/protocol/messages.ts";
import { AppState } from "../src/state/app.svelte.ts";
import { tick } from "svelte";
import { bindRootView } from "../src/views/projection.svelte.ts";
import { createRootView } from "../src/views/root.ts";

const snapshot = (): FrontendSnapshot => ({
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
});

class FakeClient {
  readonly commands: Command[] = [];
  response: Promise<ResponseResult> = Promise.resolve({ kind: "accepted" });

  request(command: Command): Promise<ResponseResult> {
    this.commands.push(command);
    return this.response;
  }
}

test("composer accepts printable text, multiline input, and terminal paste", async () => {
  const { renderer, mockInput } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  state.initialize(snapshot());
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  view.composer.input.focus();

  try {
    await mockInput.typeText("q first line");
    mockInput.pressEnter({ meta: true });
    await mockInput.pasteBracketedText("pasted\nline");

    expect(view.composer.input.plainText).toBe("q first line\npasted\nline");
  } finally {
    renderer.destroy();
  }
});

test("accepted submission clears only the submitted draft and returns focus after finish", async () => {
  const { renderer, mockInput } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  state.initialize(snapshot());
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  view.composer.input.focus();
  const client = new FakeClient();
  const interaction = new InteractionController(state, view.composer.input, client);
  view.composer.input.onSubmit = () => void interaction.dispatch("turn.submit");

  try {
    await mockInput.typeText("inspect the renderer");
    mockInput.pressEnter();
    for (let attempt = 0; attempt < 20 && client.commands.length === 0; attempt += 1) await Bun.sleep(1);
    await Bun.sleep(1);

    expect(client.commands).toEqual([{ command: "turn.submit", text: "inspect the renderer" }]);
    expect(view.composer.input.plainText).toBe("");
    expect(state.transcript.at(-1)).toMatchObject({ kind: "user", text: "inspect the renderer" });

    interaction.handleEvent({ type: "run.started" });
    view.composer.input.blur();
    interaction.handleEvent({ type: "run.finished" });
    expect(view.composer.input.focused).toBe(true);
  } finally {
    renderer.destroy();
  }
});

test("text edited while submission is pending is not cleared", async () => {
  const { renderer, mockInput } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  state.initialize(snapshot());
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  view.composer.input.focus();
  const client = new FakeClient();
  let accept: ((result: ResponseResult) => void) | undefined;
  client.response = new Promise((resolve) => {
    accept = resolve;
  });
  const interaction = new InteractionController(state, view.composer.input, client);
  view.composer.input.onSubmit = () => void interaction.submit();

  try {
    await mockInput.typeText("first draft");
    mockInput.pressEnter();
    await Bun.sleep(1);
    state.apply({ type: "status.updated", message: "unrelated update" });
    await mockInput.typeText(" plus edits");
    accept?.({ kind: "accepted" });
    await Bun.sleep(1);

    expect(view.composer.input.plainText).toBe("first draft plus edits");
  } finally {
    renderer.destroy();
  }
});

test("cancellation stays pending until the backend settles it", async () => {
  const { renderer } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  state.initialize(snapshot());
  state.apply({ type: "run.started" });
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const client = new FakeClient();
  const interaction = new InteractionController(state, view.composer.input, client);

  try {
    await interaction.cancel();
    expect(client.commands).toEqual([{ command: "turn.cancel" }]);
    expect(state.run.state).toBe("stopping");

    state.apply({ type: "status.updated", message: "waiting for worker" });
    expect(state.run.state).toBe("stopping");
    interaction.handleEvent({ type: "run.cancelled" });
    expect(state.run.state).toBe("idle");
  } finally {
    renderer.destroy();
  }
});

test("permission decisions use backend option IDs and restore composer focus", async () => {
  const { renderer } = await createTestRenderer({ width: 70, height: 18 });
  const state = new AppState();
  state.initialize({
    ...snapshot(),
    capabilities: { commands: ["permission.respond"], models: [], reasoning_efforts: [] },
  });
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  const client = new FakeClient();
  const interaction = new InteractionController(state, view.composer.input, client);
  interaction.attachOverlay(view.overlay, view.transcript, () => undefined);

  try {
    state.apply({
      type: "permission.requested",
      tool_call_id: "call-1",
      title: "Run command?",
      selected: 0,
      options: [{ id: "reject-always", name: "Always reject", kind: "reject always" }],
    });
    await tick();
    await interaction.respondPermission("reject-always");

    expect(client.commands).toEqual([
      { command: "permission.respond", tool_call_id: "call-1", option_id: "reject-always" },
    ]);
    expect(state.pendingPermission).toBeUndefined();
    expect(state.overlay).toBeUndefined();
    expect(view.composer.input.focused).toBe(true);
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("model, reasoning, context, and palette dismissal restore focus", async () => {
  const { renderer } = await createTestRenderer({ width: 70, height: 18 });
  const state = new AppState();
  state.initialize({
    ...snapshot(),
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
      commands: ["model.select", "reasoning.select"],
      models: [{ label: "fake-agent", detail: "Fake" }],
      reasoning_efforts: [
        { value: "auto", label: "Auto", description: "Default" },
        { value: "high", label: "High", description: "Difficult work" },
      ],
    },
  });
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);
  const interaction = new InteractionController(state, view.composer.input, new FakeClient());
  interaction.attachOverlay(view.overlay, view.transcript, () => undefined);

  try {
    for (const kind of ["palette", "model", "reasoning", "context"] as const) {
      interaction.openOverlay(kind);
      await tick();
      expect(state.overlay?.kind).toBe(kind);
      interaction.closeOverlay();
      expect(view.composer.input.focused).toBe(true);
    }
  } finally {
    dispose();
    renderer.destroy();
  }
});

test("submit streams and finishes through the frontend protocol", async () => {
  const { renderer } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  let interaction: InteractionController;
  const client = new FrontendClient({
    command: [process.execPath, resolve(import.meta.dir, "lifecycle-backend.ts"), "finish"],
    onEvent: (event) => interaction.handleEvent(event),
  });
  interaction = new InteractionController(state, view.composer.input, client);

  try {
    state.initialize(await client.connect());
    view.composer.input.setText("run the checks");
    await interaction.submit();
    for (
      let attempt = 0;
      attempt < 50 && !state.transcript.some((item) => item.kind === "assistant" && !item.streaming);
      attempt += 1
    )
      await Bun.sleep(2);

    expect(state.run.state).toBe("idle");
    expect(state.transcript.map((item) => item.kind)).toEqual(["user", "assistant"]);
    expect(state.transcript[1]).toMatchObject({ text: "Completed by fake backend.", streaming: false });
    expect(view.composer.input.plainText).toBe("");
  } finally {
    await client.shutdown();
    renderer.destroy();
  }
});

test("submit can be cancelled and settles through the frontend protocol", async () => {
  const { renderer } = await createTestRenderer({ width: 60, height: 12 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  let interaction: InteractionController;
  const client = new FrontendClient({
    command: [process.execPath, resolve(import.meta.dir, "lifecycle-backend.ts"), "cancel"],
    onEvent: (event) => interaction.handleEvent(event),
  });
  interaction = new InteractionController(state, view.composer.input, client);

  try {
    state.initialize(await client.connect());
    view.composer.input.setText("start a long run");
    await interaction.submit();
    for (let attempt = 0; attempt < 50 && state.run.state !== "working"; attempt += 1) await Bun.sleep(2);
    await interaction.cancel();
    for (let attempt = 0; attempt < 50 && state.run.state !== "idle"; attempt += 1) await Bun.sleep(2);

    expect(state.run.state).toBe("idle");
    expect(state.status).toBe("Cancelled");
  } finally {
    await client.shutdown();
    renderer.destroy();
  }
});

test("failed turns settle into a stable error state", () => {
  const state = new AppState();
  state.initialize(snapshot());
  state.apply({ type: "run.started" });
  state.apply({ type: "run.failed", message: "provider unavailable" });

  expect(state.run).toEqual({ state: "error", message: "provider unavailable" });
  expect(state.statusText).toContain("provider unavailable");
});
