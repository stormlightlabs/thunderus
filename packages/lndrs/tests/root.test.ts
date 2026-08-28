import { expect, test } from "bun:test";
import { createTestRenderer } from "@opentui/core/testing";
import { tick } from "svelte";
import { AppState } from "../src/state/app.svelte.ts";
import { bindRootView } from "../src/views/projection.svelte.ts";
import { createRootView } from "../src/views/root.ts";

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
    expect(captureCharFrame()).toContain("Composer arrives in LNDRS-4");

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
