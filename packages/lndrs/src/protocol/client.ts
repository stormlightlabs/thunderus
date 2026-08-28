import { NdjsonDecoder } from "./framing.ts";
import {
  type Command,
  type FrontendEvent,
  type FrontendSnapshot,
  PROTOCOL_VERSION,
  type ProtocolMessage,
  type ResponseResult,
} from "./messages.ts";

interface PendingRequest {
  resolve: (result: ResponseResult) => void;
  reject: (error: Error) => void;
}

export interface FrontendClientOptions {
  command?: string[];
  onEvent?: (event: FrontendEvent) => void;
  onExit?: (code: number) => void;
}

export class FrontendClient {
  readonly #command: string[];
  readonly #onEvent: (event: FrontendEvent) => void;
  readonly #onExit: (code: number) => void;
  readonly #pending = new Map<string, PendingRequest>();
  #nextRequest = 0;
  #closing = false;
  #process: Bun.Subprocess<"pipe", "pipe", "inherit"> | undefined;
  #readTask: Promise<void> | undefined;

  constructor(options: FrontendClientOptions = {}) {
    this.#command = options.command ?? [process.env.THNDRS_BIN ?? "thndrs", "frontend", "--stdio"];
    this.#onEvent = options.onEvent ?? (() => undefined);
    this.#onExit = options.onExit ?? (() => undefined);
  }

  async connect(): Promise<FrontendSnapshot> {
    if (this.#process) throw new Error("frontend client is already connected");

    const { THNDRS_BIN: _launcherOverride, ...backendEnvironment } = process.env;
    const child = Bun.spawn(this.#command, {
      stdin: "pipe",
      stdout: "pipe",
      stderr: "inherit",
      env: backendEnvironment,
    });
    this.#process = child;
    this.#readTask = this.#read(child.stdout).catch((error: unknown) => {
      this.#rejectAll(error instanceof Error ? error : new Error(String(error)));
      if (child.exitCode === null) child.kill();
    });
    void child.exited.then((code) => this.#handleExit(code));

    const result = await this.request({ command: "initialize", supported_versions: [PROTOCOL_VERSION] });
    if (result.kind !== "initialized" || result.protocol_version !== PROTOCOL_VERSION) {
      throw new Error("backend returned an invalid initialization response");
    }
    return result.snapshot;
  }

  async request(command: Command): Promise<ResponseResult> {
    const child = this.#process;
    if (!child) throw new Error("frontend client is not connected");

    const id = `lndrs-${++this.#nextRequest}`;
    const response = new Promise<ResponseResult>((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
    });
    const line = `${JSON.stringify({ version: PROTOCOL_VERSION, id, ...command })}\n`;
    child.stdin.write(line);
    child.stdin.flush();
    return response;
  }

  async shutdown(): Promise<void> {
    const child = this.#process;
    if (!child) return;
    this.#closing = true;

    if (child.exitCode === null) {
      try {
        await this.request({ command: "shutdown" });
      } catch {
        child.kill();
      }
    }
    child.stdin.end();
    await child.exited;
    await this.#readTask;
    this.#process = undefined;
  }

  async #read(stream: ReadableStream<Uint8Array>): Promise<void> {
    const decoder = new NdjsonDecoder();
    for await (const chunk of stream) {
      for (const message of decoder.push(chunk)) this.#handleMessage(message);
    }
    for (const message of decoder.finish()) this.#handleMessage(message);
  }

  #handleMessage(message: ProtocolMessage): void {
    if (message.type === "event") {
      this.#onEvent(message.event);
      return;
    }
    if (message.type === "protocol_error") {
      this.#rejectAll(new Error(`${message.error.code}: ${message.error.message}`));
      return;
    }

    const pending = this.#pending.get(message.id);
    if (!pending) return;
    this.#pending.delete(message.id);
    if (message.ok && message.result) {
      pending.resolve(message.result);
    } else {
      pending.reject(
        new Error(`${message.error?.code ?? "protocol_error"}: ${message.error?.message ?? "request failed"}`),
      );
    }
  }

  #handleExit(code: number): void {
    this.#rejectAll(new Error(`thndrs frontend exited with status ${code}`));
    if (!this.#closing) this.#onExit(code);
  }

  #rejectAll(error: Error): void {
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
  }
}
