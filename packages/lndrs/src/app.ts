import { createCliRenderer, type KeyEvent } from "@opentui/core";
import { flushSync } from "svelte";
import { InteractionController, globalActionForKey, type CommandClient } from "./interaction.ts";
import { FrontendClient } from "./protocol/client.ts";
import type { Command, ResponseResult } from "./protocol/messages.ts";
import { loadReplayFixture, playReplay, type ReplayTiming } from "./replay.ts";
import { AppState } from "./state/app.svelte.ts";
import { bindRootView } from "./views/projection.svelte.ts";
import { mountRootView } from "./views/root.ts";

export type LandorusOptions = { replayPath?: string; replayTiming?: ReplayTiming; width?: number; height?: number };

export async function runLandorus(options: LandorusOptions = {}): Promise<void> {
  const fixture = options.replayPath ? await loadReplayFixture(options.replayPath) : undefined;
  const width = options.width ?? fixture?.terminal.width;
  const height = options.height ?? fixture?.terminal.height;
  let finish: (() => void) | undefined;
  const done = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const renderer = await createCliRenderer({
    screenMode: "alternate-screen",
    exitOnCtrlC: true,
    clearOnShutdown: true,
    width,
    height,
    onDestroy: () => finish?.(),
  });
  await withTerminalRestoration(renderer, async () => {
    let disposeProjection: (() => void) | undefined;
    let frontendClient: FrontendClient | undefined;
    let onKeypress: ((key: KeyEvent) => void) | undefined;

    try {
      const state = new AppState();
      const view = mountRootView(renderer);
      disposeProjection = bindRootView(state, view);
      let interaction: InteractionController;
      let client: CommandClient;

      if (fixture) {
        client = new ReplayCommandClient();
      } else {
        frontendClient = new FrontendClient({
          onEvent: (event) => interaction.handleEvent(event),
          onSnapshot: (snapshot) => state.recover(snapshot),
          onExit: (code) => state.backendTerminated(`thndrs frontend exited with status ${code}`),
        });
        client = frontendClient;
      }

      interaction = new InteractionController(state, view.composer.input, client);
      interaction.attachOverlay(view.overlay, view.transcript, () => finish?.());
      view.composer.input.onSubmit = () => void interaction.dispatch("turn.submit");
      onKeypress = (key: KeyEvent) => {
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

      if (fixture) {
        state.initialize(fixture.snapshot);
        flushSync();
        await playReplay(fixture, (event) => state.apply(event), options.replayTiming ?? "immediate");
      } else {
        state.initialize(await frontendClient!.connect());
      }
      flushSync();
      await done;
    } finally {
      if (onKeypress) renderer.keyInput.off("keypress", onKeypress);
      disposeProjection?.();
      await frontendClient?.shutdown();
    }
  });
}

export async function withTerminalRestoration<T>(renderer: { destroy(): void }, action: () => Promise<T>): Promise<T> {
  try {
    return await action();
  } finally {
    renderer.destroy();
  }
}

class ReplayCommandClient implements CommandClient {
  async request(_command: Command): Promise<ResponseResult> {
    return { kind: "accepted" };
  }
}
