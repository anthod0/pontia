import { testDevice } from "./deviceFixture";
import { afterEach, expect, test, vi } from "vitest";
const { getSession, getWorkflow } = testDevice.api;

function response(data: unknown) {
  return new Response(JSON.stringify({ data }), { status: 200 });
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

test.each(["session", "workflow"])(
  "bounds stalled %s reads and allows a subsequent read",
  async (kind) => {
    vi.useFakeTimers();
    const timeout = new AbortController();
    const timeoutSpy = vi.spyOn(AbortSignal, "timeout").mockReturnValue(timeout.signal);
    const fetch = vi.fn(
      (_input, init: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(init.signal?.reason), { once: true });
        }),
    );
    vi.stubGlobal("fetch", fetch);
    const read = () => (kind === "session" ? getSession("s") : getWorkflow("wf"));
    const pending = read();
    const assertion = expect(pending).rejects.toMatchObject({ name: "TimeoutError" });
    timeout.abort(new DOMException("Read timed out", "TimeoutError"));
    await assertion;
    expect(timeoutSpy).toHaveBeenCalledWith(15_000);
    expect(fetch).toHaveBeenCalledTimes(1);
    timeoutSpy.mockReturnValue(new AbortController().signal);
    fetch.mockResolvedValueOnce(response({ session: { state: "idle" }, workflow: {} }));
    await expect(read()).resolves.toBeDefined();
  },
);

test("caller cancellation also aborts a bounded read, including a stalled response body", async () => {
  const controller = new AbortController();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (_input, init: RequestInit) => ({
      text: () =>
        new Promise((_resolve, reject) => {
          init.signal?.addEventListener("abort", () => reject(init.signal?.reason), { once: true });
        }),
    })),
  );
  const pending = getWorkflow("wf", { signal: controller.signal });
  const assertion = expect(pending).rejects.toMatchObject({ name: "AbortError" });
  await Promise.resolve();
  await Promise.resolve();
  controller.abort();
  await assertion;
});
