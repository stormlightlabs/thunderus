export {};

const baseSnapshot = {
  event_sequence: 0,
  session: { id: "gap-session", ephemeral: true, turn_count: 0 },
  workspace: process.cwd(),
  model: "fake-agent",
  reasoning_effort: "medium",
  run: { state: "idle" },
  transcript: [],
  queue: [],
  usage: { input_tokens: 0, output_tokens: 0 },
  truncated: false,
};

const decoder = new TextDecoder();
let buffer = "";
for await (const chunk of Bun.stdin.stream()) {
  buffer += decoder.decode(chunk, { stream: true });
  let newline = buffer.indexOf("\n");
  while (newline >= 0) {
    const request = JSON.parse(buffer.slice(0, newline));
    buffer = buffer.slice(newline + 1);
    if (request.command === "initialize") {
      console.log(
        JSON.stringify({
          type: "response",
          version: 1,
          id: request.id,
          ok: true,
          result: { kind: "initialized", protocol_version: 1, snapshot: baseSnapshot },
        }),
      );
      console.log(
        JSON.stringify({
          type: "event",
          version: 1,
          sequence: 1,
          event: { type: "status.updated", message: "before gap" },
        }),
      );
      console.log(
        JSON.stringify({
          type: "event",
          version: 1,
          sequence: 3,
          event: { type: "status.updated", message: "must come from snapshot" },
        }),
      );
    } else if (request.command === "state.snapshot") {
      console.log(
        JSON.stringify({
          type: "response",
          version: 1,
          id: request.id,
          ok: true,
          result: { kind: "snapshot", snapshot: { ...baseSnapshot, event_sequence: 3, model: "recovered-agent" } },
        }),
      );
    } else if (request.command === "shutdown") {
      console.log(
        JSON.stringify({ type: "response", version: 1, id: request.id, ok: true, result: { kind: "shutdown" } }),
      );
      process.exit(0);
    }
    newline = buffer.indexOf("\n");
  }
}
