export {};

const snapshot = {
  event_sequence: 0,
  session: { id: "fake-session", ephemeral: true, turn_count: 0 },
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
    const result =
      request.command === "initialize" ? { kind: "initialized", protocol_version: 1, snapshot } : { kind: "shutdown" };
    console.log(JSON.stringify({ type: "response", version: 1, id: request.id, ok: true, result }));
    if (request.command === "shutdown") process.exit(0);
    newline = buffer.indexOf("\n");
  }
}
