import { describe, it, expect } from "vitest";
import { selection } from "./selection";
describe("semantic selection", () => {
  it("pins a snapshot without preserving a pagination cursor or ticket", () => {
    expect(
      selection("?snapshot=41&signature=abc&cursor=temporary&ticket=secret"),
    ).toEqual({ snapshot: 41, scope: { signature: ["abc"] } });
  });
  it("rejects malformed sequence values without widening the selection", () => {
    expect(selection("?from_seq=bad&kind=rpc.boundary")).toEqual({
      scope: {},
      error:
        "Invalid from_seq. Use an exact safe integer for browser navigation.",
    });
  });
});
