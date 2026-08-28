import type { KeyEvent, TextareaRenderable } from "@opentui/core";
import type { Command, FrontendEvent, ResponseResult } from "./protocol/messages.ts";
import type { AppState } from "./state/app.svelte.ts";

export type FrontendAction = "turn.submit" | "composer.newline" | "turn.cancel" | "app.quit";

export interface CommandClient {
  request(command: Command): Promise<ResponseResult>;
}

export const composerKeyBindings = [
  { name: "return", action: "submit" as const },
  { name: "kpenter", action: "submit" as const },
  { name: "linefeed", action: "submit" as const },
  { name: "return", meta: true, action: "newline" as const },
  { name: "kpenter", meta: true, action: "newline" as const },
];

export function globalActionForKey(key: KeyEvent, activeRun: boolean): FrontendAction | undefined {
  if (key.name === "d" && key.ctrl) return "app.quit";
  if (key.name === "escape" && activeRun) return "turn.cancel";
  return undefined;
}

export class InteractionController {
  readonly #state: AppState;
  readonly #composer: TextareaRenderable;
  readonly #client: CommandClient;
  #submitting = false;
  #cancelling = false;

  constructor(state: AppState, composer: TextareaRenderable, client: CommandClient) {
    this.#state = state;
    this.#composer = composer;
    this.#client = client;
  }

  async dispatch(action: FrontendAction): Promise<void> {
    switch (action) {
      case "turn.submit":
        await this.submit();
        break;
      case "turn.cancel":
        await this.cancel();
        break;
      case "composer.newline":
        this.#composer.newLine();
        break;
      case "app.quit":
        break;
    }
  }

  async submit(): Promise<void> {
    const text = this.#composer.plainText;
    if (this.#submitting || this.#state.run.state !== "idle" || !text.trim()) return;
    this.#submitting = true;
    this.#state.beginSubmission();
    try {
      const result = await this.#client.request({ command: "turn.submit", text });
      if (result.kind !== "accepted") throw new Error("backend did not accept the turn");
      this.#state.acceptSubmission(text);
      if (this.#composer.plainText === text) this.#composer.clear();
    } catch (error) {
      this.#state.rejectAction(error);
    } finally {
      this.#submitting = false;
    }
  }

  async cancel(): Promise<void> {
    if (this.#cancelling || (this.#state.run.state !== "working" && this.#state.run.state !== "stopping")) return;
    this.#cancelling = true;
    this.#state.beginCancellation();
    try {
      const result = await this.#client.request({ command: "turn.cancel" });
      if (result.kind !== "accepted") throw new Error("backend did not accept cancellation");
    } catch (error) {
      this.#state.rejectAction(error);
    } finally {
      this.#cancelling = false;
    }
  }

  handleEvent(event: FrontendEvent): void {
    this.#state.apply(event);
    if (event.type === "run.finished" || event.type === "run.cancelled" || event.type === "run.failed") {
      this.#composer.focus();
    }
  }
}
