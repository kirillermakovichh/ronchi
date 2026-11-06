import { expect, it } from "vitest";
import { episodeFinding, episodeWindow, groupEpisodes } from "./episodes";
import type { Item } from "../../../shared/lib/format";
const record = (seq: number, kind: string, request = "r1"): Item => ({
  event_ref: { lab_id: "L1", event_seq: seq },
  kind,
  request_id: request,
  source: "ronchi",
  schema_version: 1,
  transaction_signature: "sig",
  recorded_at: "2026-11-06T10:00:00Z",
});
it("creates an episode before any Finding and groups rules by exact supporting application", () => {
  const a = record(3, "fault.withholding_started"),
    b = record(8, "fault.withholding_started", "r2"),
    f: Item = {
      finding_id: "f1",
      scope: { signature: "sig" },
      supporting_events: [
        { event_ref: a.event_ref, role: "observed_withholding" },
      ],
    };
  const episodes = groupEpisodes([a, b], [f]);
  expect(episodes).toHaveLength(2);
  expect(episodes.find((e) => e.id === "3")?.findings).toEqual([f]);
  expect(episodes.find((e) => e.id === "8")?.findings).toEqual([]);
});
it("does not promote planned or skipped selections into applied episodes", () => {
  expect(
    groupEpisodes(
      [
        record(1, "fault.response_loss_selected"),
        record(2, "custom.selection"),
        record(3, "fault.blockhash_skipped"),
      ],
      [],
    ),
  ).toEqual([]);
});
it("does not label a cached hash as proven expired", () => {
  const a = record(4, "fault.blockhash_returned");
  expect(episodeFinding(a, [a]).rule_id).toBe("ui.cached-blockhash");
});
it("bounds follow-ups at another fault but preserves the original request ending", () => {
  const records = [
    record(1, "rpc.registered"),
    record(2, "fault.withholding_started"),
    record(4, "rpc.registered", "r2"),
    record(5, "fault.withholding_started", "r2"),
    record(8, "rpc.registered", "r3"),
    record(10, "rpc.boundary"),
  ];
  expect(
    episodeWindow(records, records[1]).records.map(
      (r) => (r.event_ref as any).event_seq,
    ),
  ).toEqual([1, 2, 4, 10]);
});
