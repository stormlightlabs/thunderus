import type {
  FrontendCapabilities,
  FrontendEvent,
  FrontendSnapshot,
  PendingPermission,
  RunState,
  TranscriptItem,
} from "../protocol/messages.ts";

export type OverlayKind = "palette" | "permission" | "model" | "reasoning" | "context" | "queue" | "session";
export interface OverlayState {
  kind: OverlayKind;
  query: string;
}

export class AppState {
  snapshot = $state<FrontendSnapshot | undefined>();
  status = $state("Connecting to thndrs…");
  transcript = $state<TranscriptItem[]>([]);
  run = $state<RunState>({ state: "idle" });
  pendingPermission = $state<PendingPermission | undefined>();
  overlay = $state<OverlayState | undefined>();
  #liveId = 0;
  #localUserId = 0;
  #submissionTranscriptIndex: number | undefined;
  #activeAssistantId: string | undefined;
  #activeReasoningId: string | undefined;

  initialize(snapshot: FrontendSnapshot): void {
    this.replaceSnapshot(snapshot, "Connected");
  }

  recover(snapshot: FrontendSnapshot): void {
    this.replaceSnapshot(snapshot, "Recovered backend state");
  }

  replaceSnapshot(snapshot: FrontendSnapshot, status = "Connected"): void {
    this.snapshot = snapshot;
    this.transcript = snapshot.transcript;
    this.#restoreActiveStreamingIds();
    this.run = snapshot.run;
    this.pendingPermission = snapshot.pending_permission ?? undefined;
    this.overlay = this.pendingPermission ? { kind: "permission", query: "" } : undefined;
    this.status = status;
  }

  beginSubmission(): void {
    this.#submissionTranscriptIndex = this.transcript.length;
    this.status = "Sending…";
  }

  acceptSubmission(text: string): void {
    const index = this.#submissionTranscriptIndex ?? this.transcript.length;
    this.transcript.splice(index, 0, { kind: "user", id: `local-user-${++this.#localUserId}`, text });
    this.#submissionTranscriptIndex = undefined;
    this.status = this.run.state === "idle" ? "Starting…" : this.status;
  }

  beginCancellation(): void {
    this.run = { state: "stopping" };
    this.status = "Stopping…";
  }

  rejectAction(error: unknown): void {
    this.#submissionTranscriptIndex = undefined;
    const message = error instanceof Error ? error.message : String(error);
    this.run = { state: "error", message };
    this.status = message;
  }

  backendTerminated(message: string): void {
    this.finishStreaming();
    this.pendingPermission = undefined;
    this.overlay = undefined;
    this.run = { state: "error", message };
    this.status = message;
  }

  apply(event: FrontendEvent): void {
    switch (event.type) {
      case "run.started":
        if (this.run.state !== "stopping") {
          this.run = { state: "working" };
          this.status = "Working";
        }
        break;
      case "run.finished":
      case "run.cancelled":
        this.finishStreaming();
        this.run = { state: "idle" };
        this.status = event.type === "run.finished" ? "Ready" : "Cancelled";
        break;
      case "run.failed":
        this.finishStreaming();
        this.run = { state: "error", message: event.message };
        this.status = event.message;
        break;
      case "assistant.delta":
        this.#appendDelta("assistant", event.text);
        break;
      case "reasoning.delta":
        this.#appendDelta("reasoning", event.text);
        break;
      case "tool.started": {
        this.#closeActiveStreaming();
        const existing = this.transcript.findIndex((item) => item.kind === "tool" && item.id === event.id);
        const tool: TranscriptItem = {
          kind: "tool",
          id: event.id,
          name: event.name,
          arguments: event.arguments,
          status: "running",
          output: [],
        };
        if (existing >= 0) this.transcript[existing] = tool;
        else this.transcript.push(tool);
        break;
      }
      case "tool.finished": {
        const index = this.transcript.findIndex((item) => item.kind === "tool" && item.id === event.id);
        if (index >= 0) {
          this.transcript[index] = {
            ...(this.transcript[index] as Extract<TranscriptItem, { kind: "tool" }>),
            status: event.status,
            output: event.output,
          };
        }
        break;
      }
      case "usage.updated":
        if (this.snapshot) {
          this.snapshot.usage = { input_tokens: event.input_tokens, output_tokens: event.output_tokens };
        }
        break;
      case "context.updated":
        if (this.snapshot) this.snapshot.context = event.context;
        break;
      case "status.updated":
        this.status = event.message;
        break;
      case "permission.requested":
        this.pendingPermission = {
          tool_call_id: event.tool_call_id,
          title: event.title,
          selected: event.selected,
          options: event.options,
        };
        this.overlay = { kind: "permission", query: "" };
        this.status = event.title;
        break;
      case "permission.resolved":
        if (this.pendingPermission?.tool_call_id === event.tool_call_id) this.pendingPermission = undefined;
        if (this.overlay?.kind === "permission") this.overlay = undefined;
        this.status = `Permission ${event.outcome}`;
        break;
      case "model.updated":
        if (this.snapshot) {
          if (event.model) this.snapshot.model = event.model;
          const capabilities = this.capabilities;
          this.snapshot.capabilities = {
            ...capabilities,
            models: event.options.length > 0 ? event.options : capabilities.models,
            reasoning_efforts:
              event.reasoning_efforts.length > 0 ? event.reasoning_efforts : capabilities.reasoning_efforts,
          };
        }
        break;
      case "reasoning.updated":
        if (this.snapshot) this.snapshot.reasoning_effort = event.effort;
        break;
      case "snapshot.updated":
        this.replaceSnapshot(event.snapshot, event.snapshot.run.state === "working" ? "Working" : "Ready");
        break;
    }
  }

  get capabilities(): FrontendCapabilities {
    return this.snapshot?.capabilities ?? { commands: [], models: [], reasoning_efforts: [] };
  }

  supports(command: string): boolean {
    return this.capabilities.commands.includes(command);
  }

  openOverlay(kind: OverlayKind): boolean {
    if (kind === "permission" && !this.pendingPermission) return false;
    if (kind === "model" && (!this.supports("model.select") || this.capabilities.models.length === 0)) return false;
    if (kind === "reasoning" && (!this.supports("reasoning.select") || this.capabilities.reasoning_efforts.length < 2))
      return false;
    if (kind === "context" && !this.snapshot?.context) return false;
    if (kind === "queue" && !this.supports("queue.delete")) return false;
    if (kind === "session" && (!this.supports("session.load") || !this.snapshot?.sessions?.length)) return false;
    this.overlay = { kind, query: "" };
    return true;
  }

  closeOverlay(): void {
    if (this.overlay?.kind !== "permission") this.overlay = undefined;
  }

  setOverlayQuery(query: string): void {
    if (this.overlay) this.overlay.query = query;
  }

  settlePermissionLocally(): void {
    this.pendingPermission = undefined;
    if (this.overlay?.kind === "permission") this.overlay = undefined;
  }

  get statusText(): string {
    const model = this.snapshot?.model ?? "no model";
    const reasoning = this.snapshot?.reasoning_effort;
    const context = this.snapshot?.context;
    const contextText = context
      ? `${compactTokens(context.used_tokens)} / ${compactTokens(context.context_window)} · ${Math.round(
          (context.used_tokens * 100) / Math.max(context.context_window, 1),
        )}%`
      : undefined;
    const pendingQueue = this.snapshot?.queue.filter((item) => item.settlement === "pending").length ?? 0;
    const queueText = pendingQueue > 0 ? `${pendingQueue} queued` : undefined;
    const historyText = this.snapshot?.truncated ? "earlier history omitted" : undefined;
    const action =
      this.run.state === "working"
        ? "Enter follow-up · Ctrl+S steer · Esc stop"
        : this.run.state === "stopping"
          ? "stopping…"
          : "Ctrl+P commands";
    return [model, reasoning, contextText, queueText, historyText, this.status, action].filter(Boolean).join(" · ");
  }

  #appendDelta(kind: "assistant" | "reasoning", text: string): void {
    const activeId = kind === "assistant" ? this.#activeAssistantId : this.#activeReasoningId;
    const current = activeId ? this.transcript.find((item) => item.id === activeId) : undefined;
    if (current?.kind === kind && current.streaming) {
      current.text += text;
      return;
    }

    const id = `live-${++this.#liveId}`;
    this.transcript.push({ kind, id, text, streaming: true });
    if (kind === "assistant") this.#activeAssistantId = id;
    else this.#activeReasoningId = id;
  }

  #closeActiveStreaming(): void {
    for (const id of [this.#activeAssistantId, this.#activeReasoningId]) {
      const item = id ? this.transcript.find((candidate) => candidate.id === id) : undefined;
      if (item && (item.kind === "assistant" || item.kind === "reasoning")) item.streaming = false;
    }
    this.#activeAssistantId = undefined;
    this.#activeReasoningId = undefined;
  }

  #restoreActiveStreamingIds(): void {
    this.#activeAssistantId = undefined;
    this.#activeReasoningId = undefined;
    for (const item of this.transcript) {
      if (item.kind === "assistant" && item.streaming) this.#activeAssistantId = item.id;
      if (item.kind === "reasoning" && item.streaming) this.#activeReasoningId = item.id;
    }
  }

  finishStreaming(): void {
    this.#closeActiveStreaming();
  }
}

function compactTokens(tokens: number): string {
  if (tokens < 1_000) return String(tokens);
  const thousands = tokens / 1_000;
  return `${thousands >= 10 ? Math.round(thousands) : thousands.toFixed(1).replace(/\.0$/, "")}k`;
}
