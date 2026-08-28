import type { FrontendEvent, FrontendSnapshot, RunState, TranscriptItem } from "../protocol/messages.ts";

export class AppState {
  snapshot = $state<FrontendSnapshot | undefined>();
  status = $state("Connecting to thndrs…");
  transcript = $state<TranscriptItem[]>([]);
  run = $state<RunState>({ state: "idle" });
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
      case "status.updated":
        this.status = event.message;
        break;
      case "permission.requested":
        this.status = event.title;
        break;
      case "permission.resolved":
        this.status = `Permission ${event.outcome}`;
        break;
      case "model.updated":
        break;
    }
  }

  get statusText(): string {
    const model = this.snapshot?.model ?? "no model";
    const usage = this.snapshot?.usage;
    const tokens = usage ? `${usage.input_tokens + usage.output_tokens} tokens` : "0 tokens";
    const action =
      this.run.state === "working" ? "Esc stop" : this.run.state === "stopping" ? "stopping…" : "Ctrl+D quit";
    return `${model} · ${tokens} · ${this.status} · ${action}`;
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
