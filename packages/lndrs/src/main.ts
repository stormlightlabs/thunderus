import { createCliRenderer, type KeyEvent } from "@opentui/core";
import { flushSync } from "svelte";
import { FrontendClient } from "./protocol/client.ts";
import { AppState } from "./state/app.svelte.ts";
import { bindRootView } from "./views/projection.svelte.ts";
import { mountRootView } from "./views/root.ts";

async function run(): Promise<void> {
  let finish: (() => void) | undefined;
  const done = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const renderer = await createCliRenderer({
    screenMode: "alternate-screen",
    exitOnCtrlC: true,
    clearOnShutdown: true,
    onDestroy: () => finish?.(),
  });
  const state = new AppState();
  const view = mountRootView(renderer);
  const disposeProjection = bindRootView(state, view);
  const client = new FrontendClient({ onEvent: (event) => state.apply(event), onExit: () => finish?.() });
  const onKeypress = (key: KeyEvent) => {
    if (key.name === "q" && !key.ctrl && !key.meta) finish?.();
  };
  renderer.keyInput.on("keypress", onKeypress);

  try {
    state.initialize(await client.connect());
    flushSync();
    await done;
  } finally {
    renderer.keyInput.off("keypress", onKeypress);
    disposeProjection();
    renderer.destroy();
    await client.shutdown();
  }
}

try {
  await run();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
