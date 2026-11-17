import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import MessageResponse from "../../src/lib/components/ai-elements/message/message-response.svelte";
import { token } from "../../src/stores/auth";

let saved: { name: string; blob: Blob }[];
let blob: Blob;

beforeEach(() => {
  saved = [];
  token.set("download-test-token");
  vi.spyOn(URL, "createObjectURL").mockImplementation((value) => {
    blob = value as Blob;
    return "blob:download";
  });
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function () {
    saved.push({ name: this.download, blob });
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

function message(href: string) {
  return render(MessageResponse, {
    content: `Read [the file](<${href}>).`,
    markdown: true,
    sessionId: "session-1",
  });
}

test.each([
  ["/home/agent/报告%20one.md", "/home/agent/报告 one.md", "报告 one.md"],
  ["/home/agent/report.md:12:3", "/home/agent/report.md", "report.md"],
  ["/home/agent/report.md#L12", "/home/agent/report.md", "report.md"],
  ["/home/agent/report.md#L12-L15", "/home/agent/report.md", "report.md"],
  ["/home/agent/report%23L12%3A3", "/home/agent/report#L12:3", "report#L12:3"],
])("downloads the whole file from %s only when clicked", async (href, path, name) => {
  const fetch = vi
    .fn()
    .mockResolvedValue(new Response("current file", { headers: { "Content-Length": "12" } }));
  vi.stubGlobal("fetch", fetch);
  message(href);
  expect(fetch).not.toHaveBeenCalled();

  await fireEvent.click(screen.getByRole("button", { name: `Download ${name}` }));
  await waitFor(() => expect(saved).toHaveLength(1));
  const [url, options] = fetch.mock.calls[0];
  const request = new URL(url, "http://localhost");
  expect(request.pathname).toBe("/api/v1/sessions/session-1/files/content");
  expect(request.searchParams.get("path")).toBe(path);
  expect(options.headers.get("Authorization")).toBe("Bearer download-test-token");
  expect(saved[0].name).toBe(name);
  expect(await saved[0].blob.text()).toBe("current file");
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
});

test("tracks a slow download and prevents duplicate requests until it finishes", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      stream = controller;
    },
  });
  stream.enqueue(new TextEncoder().encode("first"));
  const fetch = vi
    .fn()
    .mockResolvedValue(new Response(body, { headers: { "Content-Length": "10" } }));
  vi.stubGlobal("fetch", fetch);
  message("/tmp/report.txt");
  const button = screen.getByRole("button", { name: "Download report.txt" });
  await fireEvent.click(button);
  expect(button).toBeDisabled();
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "50"),
  );
  await fireEvent.click(button);
  expect(fetch).toHaveBeenCalledTimes(1);
  stream.enqueue(new TextEncoder().encode(" last"));
  stream.close();
  await waitFor(() => expect(button).toBeEnabled());
  expect(saved).toHaveLength(1);
  expect(await saved[0].blob.text()).toBe("first last");
  expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
});

test("uses indeterminate progress when response size is unknown", async () => {
  let stream!: ReadableStreamDefaultController<Uint8Array>;
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      stream = controller;
    },
  });
  stream.enqueue(new TextEncoder().encode("partial"));
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(body)));
  message("/tmp/report.txt");
  await fireEvent.click(screen.getByRole("button"));
  const progress = await screen.findByRole("progressbar");
  expect(progress).not.toHaveAttribute("aria-valuenow");
  stream.close();
  await waitFor(() => expect(saved).toHaveLength(1));
});

test("allows retry after a missing file becomes available", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(
      new Response(JSON.stringify({ error: { code: "not_found", message: "missing" } }), {
        status: 404,
      }),
    )
    .mockResolvedValueOnce(new Response("restored", { headers: { "Content-Length": "8" } }));
  vi.stubGlobal("fetch", fetch);
  message("/tmp/report.txt");
  const button = screen.getByRole("button");
  await fireEvent.click(button);
  await screen.findByRole("alert");
  expect(button).toBeEnabled();
  expect(saved).toHaveLength(0);
  await fireEvent.click(button);
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(await saved[0].blob.text()).toBe("restored");
});

test.each(["https://example.com/file.md", "//example.com/file.md", "docs/file.md", "#section"])(
  "preserves navigation for %s",
  (href) => {
    message(href);
    expect(screen.getByRole("link")).toHaveAttribute("href", href);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  },
);
