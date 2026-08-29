import { createCliRenderer, type KeyEvent } from "@opentui/core";
import { flushSync } from "svelte";
import { InteractionController, globalActionForKey } from "./interaction.ts";
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
  let interaction: InteractionController;
  const client = new FrontendClient({
    onEvent: (event) => interaction.handleEvent(event),
    onSnapshot: (snapshot) => state.recover(snapshot),
    onExit: (code) => state.backendTerminated(`thndrs frontend exited with status ${code}`),
  });
  interaction = new InteractionController(state, view.composer.input, client);
  interaction.attachOverlay(view.overlay, view.transcript, () => finish?.());
  view.composer.input.onSubmit = () => void interaction.dispatch("turn.submit");
  const onKeypress = (key: KeyEvent) => {
    if (interaction.handleKey(key)) {
      key.preventDefault();
      key.stopPropagation();
      return;
    }
    const action = globalActionForKey(key, state.run.state === "working" || state.run.state === "stopping");
    if (!action) return;
    key.preventDefault();
    key.stopPropagation();
    if (action === "app.quit") finish?.();
    else void interaction.dispatch(action);
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
