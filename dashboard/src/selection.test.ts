import { describe, it, expect } from "vitest";
import { selection } from "./selection";
describe("semantic selection", () => {
  it("pins a snapshot without preserving a pagination cursor or ticket", () => {
    expect(
      selection("?snapshot=41&signature=abc&cursor=temporary&ticket=secret"),
    ).toEqual({ snapshot: 41, scope: { signature: ["abc"] } });
  });
  it("keeps unpinned views live and rejects malformed sequence values", () => {
    expect(selection("?from_seq=bad&kind=rpc.boundary")).toEqual({
      snapshot: undefined,
      scope: { kind: ["rpc.boundary"] },
    });
  });
});
