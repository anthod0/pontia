import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import FilePreviewHarness from "./FilePreviewHarness.svelte";
import { token } from "../../src/stores/auth";
import { get } from "svelte/store";
import { downloads } from "../../src/lib/downloads/manager";

let saved: { name: string; blob: Blob }[];
let blob: Blob;

beforeEach(() => {
  saved = [];
  token.set("preview-test-token");
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
  for (const task of get(downloads)) downloads.cancel(task.id);
  downloads.clearFinished();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

function message(href: string) {
  return render(FilePreviewHarness, { content: `Read [the file](<${href}>).` });
}

test.each([
  ["/home/agent/报告%20one.md", "/home/agent/报告 one.md", "报告 one.md"],
  ["/home/agent/report.md:12:3", "/home/agent/report.md", "report.md"],
  ["/home/agent/report.md#L12", "/home/agent/report.md", "report.md"],
  ["/home/agent/report.md#L12-L15", "/home/agent/report.md", "report.md"],
  ["/home/agent/report%23L12%3A3", "/home/agent/report#L12:3", "report#L12:3"],
])("previews the whole file from %s only when clicked", async (href, path, name) => {
  const fetch = vi.fn().mockResolvedValue(new Response("current file"));
  vi.stubGlobal("fetch", fetch);
  message(href);
  expect(fetch).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
  const panel = screen.getByRole("complementary");
  await within(panel).findByText("current file");
  const [url, options] = fetch.mock.calls[0];
  const request = new URL(url, "http://localhost");
  expect(request.pathname).toBe("/api/v1/sessions/session-1/files/content");
  expect(request.searchParams.get("path")).toBe(path);
  expect(options.headers.get("Authorization")).toBe("Bearer preview-test-token");
  expect(saved).toHaveLength(0);
});

test("renders Markdown, switches to source, copies the original and downloads on demand", async () => {
  const source = "# Report\n\n**Hello** world.\n";
  const writeText = vi.fn().mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(() => Promise.resolve(new Response(source))),
  );
  message("/tmp/report.md");
  await fireEvent.click(screen.getByRole("button", { name: "Preview report.md" }));
  await screen.findByRole("heading", { name: "Report" });
  await fireEvent.click(screen.getByRole("tab", { name: "Source" }));
  await waitFor(() =>
    expect(document.querySelector("[data-file-preview-content] pre")?.textContent).toBe(source),
  );
  await fireEvent.click(screen.getByRole("button", { name: "Copy", exact: true }));
  expect(writeText).toHaveBeenCalledWith(source);
  await fireEvent.click(screen.getByRole("button", { name: "Download report.md" }));
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(saved[0].name).toBe("report.md");
  expect(await saved[0].blob.text()).toBe(source);
});

test("cancels an old preview when another link is opened, ignores late results, and cancels on close", async () => {
  let first!: (response: Response) => void;
  const fetch = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          first = resolve;
        }),
    )
    .mockResolvedValueOnce(new Response("second content"));
  vi.stubGlobal("fetch", fetch);
  render(FilePreviewHarness, { content: "[one](/tmp/one.txt) [two](/tmp/two.txt)" });
  await fireEvent.click(screen.getByRole("button", { name: "Preview one.txt" }));
  await fireEvent.click(screen.getByRole("button", { name: "Preview two.txt" }));
  expect(fetch.mock.calls[0][1].signal.aborted).toBe(true);
  await screen.findByText("second content");
  first(new Response("stale content"));
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(screen.queryByText("stale content")).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Close", exact: true }));
  expect(fetch.mock.calls[1][1].signal.aborted).toBe(true);
  expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
});

test("allows retry after a missing file becomes available", async () => {
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ error: { code: "not_found", message: "missing" } }), {
          status: 404,
        }),
      )
      .mockResolvedValueOnce(new Response("restored")),
  );
  message("/tmp/report.txt");
  await fireEvent.click(screen.getByRole("button", { name: "Preview report.txt" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Copy", exact: true })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Download report.txt" })).toBeEnabled();
  await fireEvent.click(screen.getByRole("button", { name: "Retry", exact: true }));
  await screen.findByText("restored");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

test("keeps downloads available for a binary file without displaying its contents", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(() => Promise.resolve(new Response(new Uint8Array([0, 1, 2])))),
  );
  message("/tmp/data.bin");
  await fireEvent.click(screen.getByRole("button", { name: "Preview data.bin" }));
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Copy", exact: true })).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "Download data.bin" }));
  await waitFor(() => expect(saved).toHaveLength(1));
  expect(new Uint8Array(await saved[0].blob.arrayBuffer())).toEqual(new Uint8Array([0, 1, 2]));
});

test.each(["https://example.com/file.md", "//example.com/file.md", "docs/file.md", "#section"])(
  "preserves navigation for %s",
  (href) => {
    message(href);
    expect(screen.getByRole("link")).toHaveAttribute("href", href);
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  },
);
