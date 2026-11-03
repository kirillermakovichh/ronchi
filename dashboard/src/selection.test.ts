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
        "Invalid from_seq. Use a decimal integer within the Journal range.",
    });
  });
});

it("round-trips a pinned event bookmark while omitting transient credentials and cursors", () => {
  const view = selection(
    "?snapshot=9007199254740991&event=41&signature=A&signature=B",
  );
  expect(view.detail).toEqual({ event_seq: 41 });
  expect(view.scope.signature).toEqual(["A", "B"]);
  expect(selection("?event=41").error).toContain("snapshot");
  expect(selection("?snapshot=4&event=9223372036854775808").error).toBeTruthy();
});

it("preserves full Journal integer coordinates without rounding", () => {
  expect(
    selection("?snapshot=9007199254740993&event=9007199254740992").detail,
  ).toEqual({ event_seq: "9007199254740992" });
  expect(selection("?snapshot=9007199254740993").snapshot).toBe(
    "9007199254740993",
  );
});
