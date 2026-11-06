import { it, expect } from "vitest";
import { requestJson } from "./wire";
it("encodes exact integer coordinates without changing decimal fact strings", () => {
  expect(
    requestJson({
      arguments: {
        snapshot_seq: "9007199254740993",
        scope: {
          event_refs: [{ lab_id: "L1", event_seq: "9007199254740992" }],
          case_activation_id: ["9007199254740994"],
        },
      },
      fact: "9007199254740993",
    }),
  ).toBe(
    '{"arguments":{"snapshot_seq":9007199254740993,"scope":{"event_refs":[{"lab_id":"L1","event_seq":9007199254740992}],"case_activation_id":[9007199254740994]}},"fact":"9007199254740993"}',
  );
});
