export {};

const mode = process.argv[2] ?? "finish";
let sequence = 0;
const snapshot = {
  event_sequence: 0,
  session: { id: "lifecycle-session", ephemeral: true, turn_count: 0 },
  workspace: process.cwd(),
  model: "fake-agent",
  reasoning_effort: "medium",
  run: { state: "idle" },
  transcript: [],
  queue: [],
  usage: { input_tokens: 0, output_tokens: 0 },
  truncated: false,
};

function response(id: string, result: unknown): void {
  console.log(JSON.stringify({ type: "response", version: 1, id, ok: true, result }));
}

function event(value: unknown): void {
  sequence += 1;
  console.log(JSON.stringify({ type: "event", version: 1, sequence, event: value }));
}

const decoder = new TextDecoder();
let buffer = "";
for await (const chunk of Bun.stdin.stream()) {
  buffer += decoder.decode(chunk, { stream: true });
  let newline = buffer.indexOf("\n");
  while (newline >= 0) {
    const request = JSON.parse(buffer.slice(0, newline));
    buffer = buffer.slice(newline + 1);
    switch (request.command) {
      case "initialize":
        response(request.id, { kind: "initialized", protocol_version: 1, snapshot });
        break;
      case "turn.submit":
        response(request.id, { kind: "accepted" });
        event({ type: "run.started" });
        event({ type: "assistant.delta", text: "Completed by fake backend." });
        if (mode === "finish") event({ type: "run.finished" });
        break;
      case "turn.cancel":
        response(request.id, { kind: "accepted" });
        event({ type: "run.cancelled" });
        break;
      case "shutdown":
        response(request.id, { kind: "shutdown" });
        process.exit(0);
    }
    newline = buffer.indexOf("\n");
  }
}
