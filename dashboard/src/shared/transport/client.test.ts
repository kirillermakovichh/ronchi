// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Api } from "./client";
class Socket {
  static OPEN = 1;
  static all: Socket[] = [];
  readyState = 1;
  bufferedAmount = 0;
  sent: any[] = [];
  onmessage?: (e: { data: string }) => void;
  onclose?: () => void;
  onerror?: () => void;
  constructor(readonly url: string) {
    Socket.all.push(this);
  }
  send(message: string) {
    this.sent.push(JSON.parse(message));
  }
  receive(frame: unknown) {
    this.onmessage?.({
      data: typeof frame === "string" ? frame : JSON.stringify(frame),
    });
  }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}
let api: Api;
beforeEach(() => {
  vi.useFakeTimers();
  Socket.all = [];
  vi.stubGlobal("WebSocket", Socket);
  api = new Api("L1");
});
afterEach(() => {
  api.dispose();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});
function ready(socket = Socket.all.at(-1)!) {
  socket.receive({ type: "ready", contract_version: 1, lab_id: "L1" });
  return socket;
}
it("uses one socket, delivers pushed exact integers, and sends no idle data requests", async () => {
  const fetcher = vi.spyOn(globalThis, "fetch"),
    received = vi.fn();
  api.subscribe("api/status", undefined, received, vi.fn());
  api.subscribe("api/cases", undefined, vi.fn(), vi.fn());
  const socket = ready(),
    subscription = socket.sent.find((f) => f.path === "api/status");
  socket.receive(
    `{"type":"update","id":"${subscription.id}","status":200,"value":{"snapshot_seq":9007199254740993}}`,
  );
  await vi.advanceTimersByTimeAsync(20);
  expect(received).toHaveBeenCalledWith({ snapshot_seq: "9007199254740993" });
  const count = socket.sent.length;
  await vi.advanceTimersByTimeAsync(10000);
  expect(socket.sent).toHaveLength(count);
  expect(Socket.all).toHaveLength(1);
  expect(fetcher).not.toHaveBeenCalled();
  fetcher.mockRestore();
});
it("resynchronizes read subscriptions after reconnect without replaying an uncertain mutation", async () => {
  const receive = vi.fn(),
    failed = vi.fn();
  api.subscribe("api/status", undefined, receive, failed);
  const first = ready();
  const command = api
    .call("api/case/arm", { case_id: "tx-response-lost" })
    .catch((e) => e);
  first.close();
  expect(String(await command)).toContain("effects: unknown");
  await vi.advanceTimersByTimeAsync(1000);
  const second = ready();
  expect(second.sent.filter((f) => f.type === "subscribe")).toHaveLength(1);
  expect(second.sent.some((f) => f.type === "call")).toBe(false);
  expect(failed).toHaveBeenCalledOnce();
});
it("unsubscribes cancelled reads and sends visibility changes once per event", () => {
  const stop = api.subscribe("api/status", undefined, vi.fn(), vi.fn());
  const socket = ready();
  Object.defineProperty(document, "hidden", {
    configurable: true,
    value: true,
  });
  document.dispatchEvent(new Event("visibilitychange"));
  expect(socket.sent.at(-1)).toEqual({ type: "visibility", visible: false });
  stop();
  expect(socket.sent.at(-1).type).toBe("unsubscribe");
  Object.defineProperty(document, "hidden", {
    configurable: true,
    value: false,
  });
});
it("correlates command failures and ignores a late reply after cancellation", async () => {
  api.start();
  const socket = ready(),
    controller = new AbortController();
  const command = api
    .query("timeline", {}, undefined, undefined, {}, controller.signal)
    .catch((e) => e);
  const frame = socket.sent.at(-1);
  controller.abort();
  expect(String(await command)).toContain("cancelled");
  expect(socket.sent.at(-1)).toEqual({ type: "cancel", id: frame.id });
  socket.receive({
    type: "reply",
    id: frame.id,
    status: 200,
    value: { bad: true },
  });
  const invalid = api.call("unknown", {}).catch((e) => e);
  socket.receive({
    type: "reply",
    id: socket.sent.at(-1).id,
    status: 400,
    value: {
      error: {
        code: "invalid_argument",
        message: "Bad selector",
        effects: "none",
      },
    },
  });
  expect(String(await invalid)).toContain("invalid_argument: Bad selector");
});

it("coalesces view updates before a frame and discards cancelled subscriptions", async () => {
  const receive = vi.fn();
  const stop = api.subscribe("api/status", undefined, receive, vi.fn());
  const socket = ready();
  const id = socket.sent.find((f) => f.type === "subscribe").id;
  for (let snapshot_seq = 1; snapshot_seq <= 100; snapshot_seq++)
    socket.receive({
      type: "update",
      id,
      status: 200,
      value: { snapshot_seq },
    });
  await vi.advanceTimersByTimeAsync(20);
  expect(receive).toHaveBeenCalledTimes(1);
  expect(receive).toHaveBeenLastCalledWith({ snapshot_seq: 100 });
  socket.receive({
    type: "update",
    id,
    status: 200,
    value: { snapshot_seq: 101 },
  });
  stop();
  await vi.advanceTimersByTimeAsync(20);
  expect(receive).toHaveBeenCalledTimes(1);
});
