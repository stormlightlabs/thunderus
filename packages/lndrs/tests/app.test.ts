import { expect, test } from "bun:test";
import { withTerminalRestoration } from "../src/app.ts";

test("restores the terminal when the application throws", async () => {
  let destroyed = false;
  const renderer = { destroy: () => (destroyed = true) };

  await expect(
    withTerminalRestoration(renderer, async () => {
      throw new Error("projection failed");
    }),
  ).rejects.toThrow("projection failed");
  expect(destroyed).toBe(true);
});
