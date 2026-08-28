import type { AppState } from "../state/app.svelte.ts";
import type { RootView } from "./root.ts";

export function bindRootView(state: AppState, view: RootView): () => void {
  return $effect.root(() => {
    $effect(() => {
      view.transcript.content = state.transcriptText;
      view.status.content = state.statusText;
    });
  });
}
