import type { AppState } from "../state/app.svelte.ts";
import type { RootView } from "./root.ts";

export function bindRootView(state: AppState, view: RootView): () => void {
  return $effect.root(() => {
    $effect(() => {
      view.transcript.reconcile(state.transcript);
    });
    $effect(() => {
      view.status.content = state.statusText;
      view.composer.root.borderColor =
        state.run.state === "working"
          ? "#c8b67c"
          : state.run.state === "stopping"
            ? "#c48f8f"
            : state.run.state === "error"
              ? "#c48f8f"
              : "#596168";
    });
  });
}
