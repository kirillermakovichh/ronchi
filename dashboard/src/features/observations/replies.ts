import {
  createEffect,
  createMemo,
  createSignal,
  onCleanup,
  untrack,
  type Accessor,
} from "solid-js";
import type { Api } from "../../shared/transport/client";
import type { Item } from "../../shared/lib/format";
import {
  lookupRequests,
  replyKey,
  resolveReply,
  type LookupReply,
} from "./model/reply";
export function createReplies(
  api: Api,
  options: Accessor<{
    records: Item[];
    signature?: string;
    snapshot: number | string;
    active: boolean;
    selection: string;
    epoch?: unknown;
  }>,
) {
  const [cache, setCache] = createSignal<Map<string, LookupReply>>(new Map());
  const tasks = createMemo(() => {
    const o = options();
    return lookupRequests(o.records)
      .slice(-16)
      .map((request) => {
        const r: Item = { ...request, selected_signature: o.signature ?? "" };
        return { request: r, key: o.selection + replyKey(r, o.records) };
      });
  });
  const key = createMemo(() => JSON.stringify(tasks().map((t) => t.key)));
  createEffect(() => {
    key();
    const o = options();
    o.epoch;
    if (!o.active) return;
    const controller = new AbortController();
    onCleanup(() => controller.abort());
    const work = untrack(() =>
      tasks().filter(
        (t) => !cache().has(t.key) || cache().get(t.key)?.retryable,
      ),
    );
    let next = 0;
    const worker = async () => {
      while (next < work.length && !controller.signal.aborted) {
        const task = work[next++];
        let result: LookupReply;
        try {
          result = await resolveReply(
            api,
            task.request,
            o.records,
            o.snapshot,
            controller.signal,
          );
        } catch (e) {
          result = {
            request: task.request,
            at: task.request,
            records: [],
            label: "Reply unavailable",
            tone: "neutral",
            reason: String(e),
            retryable: true,
          };
        }
        if (controller.signal.aborted) return;
        setCache((previous) => {
          const c = new Map(previous);
          c.set(task.key, result);
          while (c.size > 32) c.delete(c.keys().next().value!);
          return c;
        });
      }
    };
    void Promise.all([worker(), worker()]);
  });
  return {
    items: (): LookupReply[] =>
      tasks().map(
        (t) =>
          cache().get(t.key) ?? {
            request: t.request,
            at: t.request,
            records: [],
            label: "Reply unavailable",
            tone: "neutral" as const,
            reason: "Reading retained reply",
          },
      ),
    loading: () => tasks().some((t) => !cache().has(t.key)),
    omitted: () => Math.max(0, lookupRequests(options().records).length - 16),
  };
}
