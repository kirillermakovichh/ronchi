import { createSignal, onCleanup } from "solid-js";
export function createAction() {
  const [busy, setBusy] = createSignal(false),
    [error, setError] = createSignal("");
  let controller: AbortController | undefined;
  onCleanup(() => controller?.abort());
  return {
    busy,
    error,
    clear: () => setError(""),
    cancel() {
      controller?.abort();
    },
    async run(work: (signal: AbortSignal) => Promise<void>) {
      if (busy()) return;
      controller = new AbortController();
      setBusy(true);
      setError("");
      try {
        await work(controller.signal);
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(false);
      }
    },
  };
}
