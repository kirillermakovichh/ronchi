import { expect, it, vi } from "vitest";
import { Api } from "../../../shared/transport/client";
import { classifyReply, resolveReply } from "./reply";
import type { Item } from "../../../shared/lib/format";
const request: Item = {
  event_ref: { lab_id: "L1", event_seq: 1 },
  kind: "rpc.registered",
  request_id: "r1",
  rpc_method: "getSignatureStatuses",
  source: "ronchi",
  schema_version: 1,
  transaction_signatures: ["other", "target"],
  selected_signature: "target",
};
const record = (seq: number, kind: string, data: Item = {}): Item => ({
  event_ref: { lab_id: "L1", event_seq: seq },
  request_id: "r1",
  kind,
  source: kind === "rpc.backend" ? "surfpool" : "ronchi",
  schema_version: 1,
  data,
  payload_status: "available",
});
const boundary = record(4, "rpc.boundary", {
  rpc_response: "result",
  transport: "http_write_completed",
});
const body = (entry: unknown) => ({ result: { value: [null, entry] } });
it.each(["confirmed", "finalized", "processed"])(
  "displays actual %s and null err, preserving signature position",
  (status) => {
    const reply = classifyReply(
      request,
      [boundary],
      body({ confirmationStatus: status, err: null }),
    );
    expect(reply.status).toBe(status);
    expect(reply.err).toBeNull();
    expect(reply.tone).toBe(status === "processed" ? "pending" : "success");
  },
);
it("separates an execution error from finality and RPC failure", () => {
  const failed = classifyReply(
    request,
    [boundary],
    body({
      confirmationStatus: "finalized",
      err: { InstructionError: [2, { Custom: 6001 }] },
    }),
  );
  expect(failed.tone).toBe("failure");
  expect(failed.label).toBe(
    "Execution error: Instruction 2: custom error 6001",
  );
  const rpc = classifyReply(request, [boundary], {
    error: { code: -32005, message: "Node unavailable" },
  });
  expect(rpc.tone).toBe("neutral");
  expect(rpc.label).toContain("RPC error");
  expect(classifyReply(request, [boundary], body(null)).label).toBe(
    "Status not found (null)",
  );
  expect(classifyReply(request, [boundary], {}).label).toBe(
    "Reply unavailable",
  );
});
it("never guesses the array position or a missing execution error field", () => {
  expect(
    classifyReply(
      { ...request, selected_signature: "absent" },
      [],
      body({ confirmationStatus: "finalized", err: null }),
    ).tone,
  ).toBe("neutral");
  expect(
    classifyReply(request, [], {
      result: { value: [{ confirmationStatus: "finalized", err: null }] },
    }).tone,
  ).toBe("neutral");
  expect(
    classifyReply(request, [], body({ confirmationStatus: "finalized" }))
      .status,
  ).toBeUndefined();
});
it("getTransaction reports its actual meta.err without inventing finality", () => {
  const r = {
    ...request,
    rpc_method: "getTransaction",
    transaction_signatures: ["target"],
  };
  expect(
    classifyReply(r, [], { result: { meta: { err: null } } }).status,
  ).toBeUndefined();
  expect(classifyReply(r, [], { result: { meta: { err: null } } }).label).toBe(
    "Transaction returned · err: null",
  );
  expect(
    classifyReply(r, [], {
      result: { meta: { err: "InsufficientFundsForFee" } },
    }).tone,
  ).toBe("failure");
  expect(classifyReply(r, [], { result: null }).label).toContain(
    "Transaction not found",
  );
});
function sourceFixture(raw: unknown) {
  const text = JSON.stringify(raw),
    encoded = btoa(String.fromCharCode(...new TextEncoder().encode(text)));
  const source = record(2, "rpc.backend", {
    source_identity: "source",
    chunk_schema_version: 2,
    source_result: {
      chunk: 0,
      chunks: 1,
      bytes: new TextEncoder().encode(text).length,
      encoding: "base64-json",
      redacted: false,
    },
  });
  const prepared = record(3, "rpc.response_prepared", {
    source_identity: "source",
    source_result: {
      kind: "rpc.backend",
      availability: "separate_source_exchange",
    },
  });
  const api = new Api("L1");
  const query = vi.spyOn(api, "query").mockImplementation(
    async (_o, scope, _n, _c, extra) =>
      ({
        result: {
          items: [
            {
              event_ref: scope.event_refs![0],
              payload: {
                preview: encoded.slice(
                  Number(extra?.payload_offset ?? 0),
                  Number(extra?.payload_offset ?? 0) + 4096,
                ),
                offset: extra?.payload_offset ?? 0,
                retained_bytes: encoded.length,
                next_offset:
                  Number(extra?.payload_offset ?? 0) + 4096 < encoded.length
                    ? Number(extra?.payload_offset ?? 0) + 4096
                    : null,
              },
            },
          ],
        },
      }) as any,
  );
  return {
    api,
    query,
    source,
    prepared,
    rows: [request, source, prepared, boundary],
  };
}
it("reads a retained backend result through exact event/snapshot payload calls", async () => {
  const f = sourceFixture(body({ confirmationStatus: "finalized", err: null }));
  const result = await resolveReply(
    f.api,
    request,
    f.rows,
    4,
    new AbortController().signal,
  );
  expect(result.label).toBe("finalized · err: null");
  expect(result.records.map((r) => r.event_ref)).toContainEqual(
    f.source.event_ref,
  );
  expect(f.query.mock.calls[0][1]).toEqual({
    event_refs: [f.source.event_ref],
  });
  expect(f.query.mock.calls[0][2]).toBe(4);
});
it("follows bounded byte previews and decodes only a complete retained response", async () => {
  const f = sourceFixture({
    ...body({ confirmationStatus: "confirmed", err: null }),
    padding: "x".repeat(8000),
  });
  expect(
    (
      await resolveReply(
        f.api,
        request,
        f.rows,
        4,
        new AbortController().signal,
      )
    ).status,
  ).toBe("confirmed");
  expect(f.query.mock.calls.length).toBeGreaterThan(1);
  f.query.mockResolvedValue({ result: { items: [] } } as any);
  expect(
    (
      await resolveReply(
        f.api,
        request,
        f.rows,
        4,
        new AbortController().signal,
      )
    ).status,
  ).toBeUndefined();
});
it("does not read redacted/oversized/inconsistent source payloads or imply delivery from prepared state", async () => {
  for (const change of ["redacted", "oversized", "identity", "chunks"]) {
    const f = sourceFixture(
      body({ confirmationStatus: "finalized", err: null }),
    );
    if (change === "redacted") f.source.payload_status = "redacted";
    if (change === "oversized")
      (f.source.data as any).source_result.bytes = 40000;
    if (change === "identity") (f.source.data as any).source_identity = "wrong";
    if (change === "chunks") (f.source.data as any).source_result.chunks = 2;
    expect(
      (
        await resolveReply(
          f.api,
          request,
          f.rows,
          4,
          new AbortController().signal,
        )
      ).status,
    ).toBeUndefined();
    expect(f.query).not.toHaveBeenCalled();
  }
  const f = sourceFixture(body({ confirmationStatus: "finalized", err: null }));
  expect(
    (
      await resolveReply(
        f.api,
        request,
        f.rows.filter((r) => r !== boundary),
        4,
        new AbortController().signal,
      )
    ).label,
  ).toBe("Response pending");
  expect(f.query).not.toHaveBeenCalled();
  expect(
    (
      await resolveReply(
        f.api,
        request,
        [
          record(3, "rpc.response_prepared", {
            result: (
              body({ confirmationStatus: "finalized", err: null }) as any
            ).result,
          }),
          record(4, "rpc.boundary", {
            transport: "transport_failed",
            rpc_response: "result",
          }),
        ],
        4,
        new AbortController().signal,
      )
    ).label,
  ).toBe("Delivery unconfirmed");
});
it("uses a verified application-facing null rather than the source's finalized status", async () => {
  try {
    const f = sourceFixture(
      body({ confirmationStatus: "finalized", err: null }),
    );
    const hash = [
      ...new Uint8Array(
        await crypto.subtle.digest("SHA-256", new TextEncoder().encode("null")),
      ),
    ]
      .map((n) => n.toString(16).padStart(2, "0"))
      .join("");
    (f.prepared.data as any).lookup_boundary_digests = [hash, hash];
    const effect = record(2.5, "fault.lookup_changed", {
      entry_index: 1,
      returned_view: null,
    });
    // Event coordinates are integers in the Journal.
    effect.event_ref = { lab_id: "L1", event_seq: 2 };
    const result = await resolveReply(
      f.api,
      request,
      [request, effect, f.prepared, boundary],
      4,
      new AbortController().signal,
    );
    expect(result.label).toBe("Status not found (null)");
    expect(result.raw).toMatchObject({
      availability: "verified_entry_only",
      returned_view: null,
    });
    expect(f.query).not.toHaveBeenCalled();
    (f.prepared.data as any).lookup_boundary_digests = ["mismatch", "mismatch"];
    expect(
      (
        await resolveReply(
          f.api,
          request,
          [request, effect, f.prepared, boundary],
          4,
          new AbortController().signal,
        )
      ).label,
    ).toBe("Reply unavailable");
  } finally {
    vi.unstubAllGlobals();
  }
});
it("checks source digests instead of exposing an unverified changed view", async () => {
  try {
    const f = sourceFixture(
      body({ confirmationStatus: "finalized", err: null }),
    );
    (f.prepared.data as any).lookup_boundary_digests = ["wrong", "wrong"];
    expect(
      (
        await resolveReply(
          f.api,
          request,
          f.rows,
          4,
          new AbortController().signal,
        )
      ).status,
    ).toBeUndefined();
  } finally {
    vi.unstubAllGlobals();
  }
});

it("does not attribute a transaction reply to a missing or different signature", () => {
  for (const signatures of [[], ["other"], ["target", "other"]]) {
    const reply = classifyReply(
      {
        ...request,
        rpc_method: "getTransaction",
        transaction_signatures: signatures,
      },
      [],
      { result: { meta: { err: "InstructionError" } } },
    );
    expect(reply.tone).toBe("neutral");
    expect(reply.err).toBeUndefined();
  }
});

it("distinguishes a recorded absent response from uncertain delivery at the actual boundary", async () => {
  const api = new Api("L1");
  const ending = record(4, "rpc.boundary", {
    rpc_response: "no_transport_response",
    transport: "withholding_deadline",
  });
  const reply = await resolveReply(
    api,
    request,
    [request, record(3, "rpc.response_prepared"), ending],
    4,
    new AbortController().signal,
  );
  expect(reply.label).toBe("No RPC reply");
  expect(reply.tone).toBe("neutral");
  expect(reply.at.event_ref).toEqual(ending.event_ref);
});
