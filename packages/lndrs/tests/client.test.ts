import { expect, test } from "bun:test";
import { resolve } from "node:path";
import { FrontendClient } from "../src/protocol/client.ts";
import type { FrontendSnapshot } from "../src/protocol/messages.ts";

test("spawns a backend and completes the protocol handshake", async () => {
  const client = new FrontendClient({ command: [process.execPath, resolve(import.meta.dir, "fake-backend.ts")] });

  const snapshot = await client.connect();
  expect(snapshot.session.id).toBe("fake-session");
  expect(snapshot.model).toBe("fake-agent");
  await client.shutdown();
});

test("recovers atomically after an event sequence gap", async () => {
  const events: string[] = [];
  let recovered: FrontendSnapshot | undefined;
  const client = new FrontendClient({
    command: [process.execPath, resolve(import.meta.dir, "sequence-gap-backend.ts")],
    onEvent: (event) => events.push(event.type),
    onSnapshot: (snapshot) => {
      recovered = snapshot;
    },
  });

  await client.connect();
  for (let attempt = 0; attempt < 50 && !recovered; attempt += 1) await Bun.sleep(2);

  expect(events).toEqual(["status.updated"]);
  expect(recovered?.event_sequence).toBe(3);
  expect(recovered?.model).toBe("recovered-agent");
  await client.shutdown();
});
