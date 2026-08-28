import { type ProtocolMessage, ProtocolParseError, parseProtocolMessage } from "./messages.ts";

const MAX_LINE_BYTES = 1024 * 1024;

export class NdjsonDecoder {
  readonly #decoder = new TextDecoder("utf-8", { fatal: true });
  #buffer = "";

  push(chunk: Uint8Array): ProtocolMessage[] {
    this.#buffer += this.#decoder.decode(chunk, { stream: true });
    if (this.#buffer.length > MAX_LINE_BYTES && !this.#buffer.includes("\n")) {
      throw new ProtocolParseError("protocol line exceeds 1 MiB");
    }
    return this.#drainLines();
  }

  finish(): ProtocolMessage[] {
    this.#buffer += this.#decoder.decode();
    const messages = this.#drainLines();
    if (this.#buffer.trim().length > 0) {
      messages.push(this.#parse(this.#buffer));
    }
    this.#buffer = "";
    return messages;
  }

  #drainLines(): ProtocolMessage[] {
    const messages: ProtocolMessage[] = [];
    let newline = this.#buffer.indexOf("\n");
    while (newline >= 0) {
      const line = this.#buffer.slice(0, newline).replace(/\r$/, "");
      this.#buffer = this.#buffer.slice(newline + 1);
      if (line.trim().length > 0) {
        messages.push(this.#parse(line));
      }
      newline = this.#buffer.indexOf("\n");
    }
    return messages;
  }

  #parse(line: string): ProtocolMessage {
    if (new TextEncoder().encode(line).byteLength > MAX_LINE_BYTES) {
      throw new ProtocolParseError("protocol line exceeds 1 MiB");
    }
    try {
      return parseProtocolMessage(JSON.parse(line));
    } catch (error) {
      if (error instanceof ProtocolParseError) throw error;
      throw new ProtocolParseError(`invalid protocol JSON: ${String(error)}`);
    }
  }
}
