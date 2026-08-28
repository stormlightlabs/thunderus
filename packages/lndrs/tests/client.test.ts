import { expect, test } from "bun:test";
import { resolve } from "node:path";
import { FrontendClient } from "../src/protocol/client.ts";

test("spawns a backend and completes the protocol handshake", async () => {
  const client = new FrontendClient({ command: [process.execPath, resolve(import.meta.dir, "fake-backend.ts")] });

  const snapshot = await client.connect();
  expect(snapshot.session.id).toBe("fake-session");
  expect(snapshot.model).toBe("fake-agent");
  await client.shutdown();
});
