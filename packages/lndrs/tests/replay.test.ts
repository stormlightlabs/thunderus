import { expect, test } from "bun:test";
import { createTestRenderer } from "@opentui/core/testing";
import { tick } from "svelte";
import type { FrontendEvent, FrontendSnapshot } from "../src/protocol/messages.ts";
import { AppState } from "../src/state/app.svelte.ts";
import { bindRootView } from "../src/views/projection.svelte.ts";
import { createRootView } from "../src/views/root.ts";

interface ReplayFixture {
  snapshot: FrontendSnapshot;
  events: FrontendEvent[];
}

test("replays a long tool-heavy run into retained, culled blocks", async () => {
  const fixture = (await Bun.file(`${import.meta.dir}/fixtures/tool-heavy.json`).json()) as ReplayFixture;
  const { renderer, renderOnce, captureCharFrame } = await createTestRenderer({ width: 54, height: 16 });
  const state = new AppState();
  const view = createRootView(renderer);
  renderer.root.add(view.root);
  const dispose = bindRootView(state, view);

  try {
    state.initialize(fixture.snapshot);
    for (const event of fixture.events) state.apply(event);
    await tick();
    await renderOnce();

    expect(view.transcript.scroll.viewportCulling).toBe(true);
    expect(view.transcript.blockCount).toBe(17);
    expect(view.transcript.getBlock("call-01")?.root.id).toBe("call-01");
    expect(view.transcript.getBlock("call-12")?.root.id).toBe("call-12");
    expect(captureCharFrame()).toContain("checks pass.");
    expect(view.transcript.scroll.scrollHeight).toBeGreaterThan(view.transcript.scroll.viewport.height);
  } finally {
    dispose();
    renderer.destroy();
  }
});
