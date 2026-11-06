import { expect, it } from "vitest";
import { readRoute, routeUrl } from "./route";
it("opens Workspace by default and reads older page bookmarks", () => {
  expect(readRoute("").page).toBe("workspace");
  expect(readRoute("?page=traffic&event=12").target?.event_seq).toBe(12);
  expect(
    readRoute("?page=observations&finding=f1-" + "a".repeat(64)).page,
  ).toBe("workspace");
});
it("preserves exact coordinates, fixed scope and episode bookmarks", () => {
  const route = readRoute(
    "?snapshot=9007199254740993&episode=12&signature=abc",
  );
  const url = routeUrl("L1", route);
  expect(readRoute(new URL(url).search)).toEqual(route);
  expect(route.snapshot).toBe("9007199254740993");
  expect(url).not.toContain("ticket");
});
it("rejects duplicate and malformed selections", () => {
  expect(readRoute("?episode=1&event=2").error).toBeTruthy();
  expect(readRoute("?snapshot=nope").error).toBeTruthy();
});
