import { expect, test } from "bun:test";
import { parseArguments } from "../src/cli.ts";

test("parses deterministic replay options", () => {
  expect(
    parseArguments([
      "--replay",
      "../../crates/thndrs/tests/fixtures/frontend-replay/streaming.json",
      "--replay-timing",
      "timed",
      "--width",
      "90",
      "--height",
      "28",
    ]),
  ).toEqual({
    replayPath: "../../crates/thndrs/tests/fixtures/frontend-replay/streaming.json",
    replayTiming: "timed",
    width: 90,
    height: 28,
  });
});

test("rejects invalid replay options", () => {
  expect(() => parseArguments(["--replay-timing", "timed"])).toThrow("requires --replay");
  expect(() => parseArguments(["--replay", "fixture.json", "--width", "0"])).toThrow("positive integer");
  expect(() => parseArguments(["--unknown"])).toThrow("unsupported argument");
});
