import { expect, it } from "vitest";
import { parseEvidence } from "./json";
it("preserves unsafe integer facts without changing numeric strings or safe metadata", () => {
  expect(
    parseEvidence(
      '{"event_seq":42,"amount":9007199254740993,"negative":-9007199254740993,"text":"9007199254740993\\\""}',
    ),
  ).toEqual({
    event_seq: 42,
    amount: "9007199254740993",
    negative: "-9007199254740993",
    text: '9007199254740993"',
  });
});
