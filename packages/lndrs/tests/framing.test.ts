import { describe, expect, test } from "bun:test";
import { NdjsonDecoder } from "../src/protocol/framing.ts";
import { ProtocolParseError } from "../src/protocol/messages.ts";

describe("NDJSON framing", () => {
  test("buffers partial messages and parses multiple lines", () => {
    const decoder = new NdjsonDecoder();
    expect(decoder.push(Buffer.from('{"type":"event","version":1,"sequence":1,"event":'))).toEqual([]);
    const messages = decoder.push(
      Buffer.from(
        '{"type":"run.started"}}\n{"type":"response","version":1,"id":"x","ok":true,"result":{"kind":"accepted"}}\n',
      ),
    );

    expect(messages).toHaveLength(2);
    expect(messages[0]?.type).toBe("event");
    expect(messages[1]?.type).toBe("response");
  });

  test("rejects unknown event types", () => {
    const decoder = new NdjsonDecoder();
    expect(() =>
      decoder.push(Buffer.from('{"type":"event","version":1,"sequence":1,"event":{"type":"future.event"}}\n')),
    ).toThrow(ProtocolParseError);
  });
});
