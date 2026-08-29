import type { KeyEvent, TextareaRenderable } from "@opentui/core";
import type { Command, FrontendEvent, ResponseResult } from "./protocol/messages.ts";
import type { AppState } from "./state/app.svelte.ts";
import type { OverlayView } from "./views/overlay.ts";
import type { TranscriptView } from "./views/transcript.ts";

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
  #overlay: OverlayView | undefined;
  #transcript: TranscriptView | undefined;
  #quit: (() => void) | undefined;

  constructor(state: AppState, composer: TextareaRenderable, client: CommandClient) {
    this.#state = state;
    this.#composer = composer;
    this.#client = client;
  }

  attachOverlay(overlay: OverlayView, transcript: TranscriptView, quit: () => void): void {
    this.#overlay = overlay;
    this.#transcript = transcript;
    this.#quit = quit;
    overlay.search.on("input", (value: string) => this.#state.setOverlayQuery(value));
    overlay.search.on("enter", () => void this.selectOverlayItem());
    overlay.list.on("itemSelected", () => void this.selectOverlayItem());
  }

  handleKey(key: KeyEvent): boolean {
    if ((key.ctrl || key.meta) && key.name === "p") {
      if (this.#state.overlay?.kind === "palette") this.closeOverlay();
      else this.openOverlay("palette");
      return true;
    }

    const overlay = this.#state.overlay;
    if (!overlay) return false;
    if (key.name === "escape") {
      if (overlay.kind === "permission") {
        void this.respondPermission(null).catch((error: unknown) => {
          this.#state.status = error instanceof Error ? error.message : String(error);
        });
      } else this.closeOverlay();
      return true;
    }
    if (overlay.kind === "context") return true;
    if (overlay.kind === "palette" && key.name === "up") {
      this.#overlay?.list.moveUp();
      return true;
    }
    if (overlay.kind === "palette" && key.name === "down") {
      this.#overlay?.list.moveDown();
      return true;
    }
    return false;
  }

  openOverlay(kind: "palette" | "model" | "reasoning" | "context"): void {
    if (!this.#state.openOverlay(kind)) return;
    if (kind === "palette" && this.#overlay) this.#overlay.search.value = "";
    this.#composer.blur();
  }

  closeOverlay(): void {
    this.#state.closeOverlay();
    this.#composer.focus();
  }

  async selectOverlayItem(): Promise<void> {
    const overlay = this.#state.overlay;
    const selected = this.#overlay?.list.getSelectedOption();
    if (!overlay || !selected) return;
    const value = String(selected.value ?? "");
    try {
      switch (overlay.kind) {
        case "palette":
          if (value === "bottom") {
            this.#transcript?.scroll.scrollTo(this.#transcript.scroll.scrollHeight);
            this.closeOverlay();
          } else if (value === "quit") {
            this.#quit?.();
          } else if (value === "context" || value === "model" || value === "reasoning") {
            this.openOverlay(value);
          }
          break;
        case "permission":
          await this.respondPermission(value);
          break;
        case "model": {
          const result = await this.#client.request({ command: "model.select", model: value });
          if (result.kind !== "accepted") throw new Error("backend did not accept model selection");
          this.closeOverlay();
          break;
        }
        case "reasoning": {
          const result = await this.#client.request({ command: "reasoning.select", effort: value });
          if (result.kind !== "accepted") throw new Error("backend did not accept reasoning selection");
          this.closeOverlay();
          break;
        }
        case "context":
          break;
      }
    } catch (error) {
      this.#state.status = error instanceof Error ? error.message : String(error);
    }
  }

  async respondPermission(optionId: string | null): Promise<void> {
    const permission = this.#state.pendingPermission;
    if (!permission || !this.#state.supports("permission.respond")) return;
    const result = await this.#client.request({
      command: "permission.respond",
      tool_call_id: permission.tool_call_id,
      option_id: optionId,
    });
    if (result.kind !== "accepted") throw new Error("backend did not accept permission response");
    this.#state.settlePermissionLocally();
    this.#composer.focus();
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
