import type { FrontendEvent, FrontendSnapshot, RunState, TranscriptItem } from "../protocol/messages.ts";

export class AppState {
  snapshot = $state<FrontendSnapshot | undefined>();
  status = $state("Connecting to thndrs…");
  transcript = $state<TranscriptItem[]>([]);
  run = $state<RunState>({ state: "idle" });
  #liveId = 0;

  initialize(snapshot: FrontendSnapshot): void {
    this.snapshot = snapshot;
    this.transcript = snapshot.transcript;
    this.run = snapshot.run;
    this.status = "Connected";
  }

  apply(event: FrontendEvent): void {
    switch (event.type) {
      case "run.started":
        this.run = { state: "working" };
        this.status = "Working";
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
      case "tool.started":
        this.transcript.push({
          kind: "tool",
          id: event.id,
          name: event.name,
          arguments: event.arguments,
          status: "running",
          output: [],
        });
        break;
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

  get transcriptText(): string {
    if (this.transcript.length === 0) {
      return "Landorus\n\nFrontend connected. Transcript output will appear here.";
    }
    return this.transcript
      .map((item) => {
        switch (item.kind) {
          case "user":
            return `You\n${item.text}`;
          case "assistant":
            return `Assistant\n${item.text}`;
          case "reasoning":
            return `Reasoning\n${item.text}`;
          case "tool":
            return `Tool · ${item.name} · ${item.status}\n${item.output.join("\n") || item.arguments}`;
          case "skill":
            return `Skill · ${item.name}\n${item.path}`;
          case "status":
          case "error":
            return item.text;
        }
        return "";
      })
      .join("\n\n");
  }

  get statusText(): string {
    const model = this.snapshot?.model ?? "no model";
    const usage = this.snapshot?.usage;
    const tokens = usage ? `${usage.input_tokens + usage.output_tokens} tokens` : "0 tokens";
    return `${model} · ${tokens} · ${this.status} · q quit`;
  }

  #appendDelta(kind: "assistant" | "reasoning", text: string): void {
    const current = this.transcript.at(-1);
    if (current?.kind === kind && current.streaming) {
      current.text += text;
      return;
    }
    this.transcript.push({ kind, id: `live-${++this.#liveId}`, text, streaming: true });
  }

  finishStreaming(): void {
    for (const item of this.transcript) {
      if ((item.kind === "assistant" || item.kind === "reasoning") && item.streaming) {
        item.streaming = false;
      }
    }
  }
}
