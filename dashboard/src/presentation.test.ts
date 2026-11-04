import { expect, it } from "vitest";
import {
  boundaryResult,
  duration,
  findingTitle,
  signature,
} from "./presentation";
it("never turns a prepared response or a missing boundary into client receipt", () => {
  expect(
    boundaryResult({ exchange: { prepared: { data: { result: "ok" } } } })
      .label,
  ).toBe("Boundary not recorded");
  expect(
    boundaryResult({
      exchange: {
        boundary: {
          data: {
            rpc_response: "result",
            transport: "transport_failed",
            client_receipt: "unknown",
          },
        },
      },
    }).label,
  ).toBe("Result prepared");
  expect(
    boundaryResult({
      exchange: {
        boundary: {
          data: { rpc_response: "result", transport: "http_write_completed" },
        },
      },
    }).label,
  ).toBe("Server write completed");
  expect(
    boundaryResult({ exchange: { withholding: { data: {} } } }).label,
  ).toBe("Withholding started");
});
it("preserves exact elapsed coordinates and finds the accepted signature", () => {
  expect(
    duration({
      elapsed_ns: "9007199254740993000",
      exchange: { boundary: { elapsed_ns: "9007199254742993000" } },
    }),
  ).toBe("2 ms");
  expect(
    signature({
      exchange: { acceptance: { transaction_signature: "original" } },
    }),
  ).toBe("original");
});
it("uses readable statements for unknown Custom Finding rules", () => {
  expect(
    findingTitle({
      rule_id: "custom.opaque",
      finding_id: "opaque-id",
      statement: "A later lookup was observed.",
    }),
  ).toBe("A later lookup was observed.");
});
