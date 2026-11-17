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
  expect(await screen.findByRole("alert")).toHaveTextContent("File no longer exists");
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
  expect(await screen.findByRole("alert")).toHaveTextContent("Cannot preview this file");
  expect(screen.getByRole("button", { name: "Copy", exact: true })).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "Download", exact: true }));
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

test("shows a loading placeholder and an empty-file state", async () => {
  let resolve!: (response: Response) => void;
  vi.stubGlobal(
    "fetch",
    vi.fn().mockReturnValue(new Promise<Response>((done) => (resolve = done))),
  );
  message("/tmp/empty.txt");
  await fireEvent.click(screen.getByRole("button", { name: "Preview empty.txt" }));
  expect(screen.getByRole("status", { name: "Loading preview…" })).toBeInTheDocument();
  resolve(new Response(""));
  await screen.findByText("This file is empty");
  expect(screen.queryByRole("status", { name: "Loading preview…" })).not.toBeInTheDocument();
});

test("resizes the preview with the keyboard and remembers the width", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("content")));
  localStorage.removeItem("pontia.filePreviewWidth");
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 400,
  } as DOMRect);
  vi.stubGlobal("innerWidth", 1440);
  message("/tmp/report.txt");
  await fireEvent.click(screen.getByRole("button", { name: "Preview report.txt" }));
  const handle = screen.getByRole("separator", { name: "Resize preview" });
  await fireEvent.keyDown(handle, { key: "ArrowLeft" });
  expect(localStorage.getItem("pontia.filePreviewWidth")).toBe("432");
  expect(handle).toHaveAttribute("aria-valuenow", "432");
  await fireEvent.dblClick(handle);
  expect(localStorage.getItem("pontia.filePreviewWidth")).toBeNull();
});

function requestedPath(call: unknown[]) {
  return new URL(call[0] as string, "http://localhost").searchParams.get("path");
}

test("resolves relative chat links against the session workspace", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response("spec"));
  vi.stubGlobal("fetch", fetch);
  render(FilePreviewHarness, {
    content: "[spec](docs/spec%20v1.md) [up](../other/notes.md) [site](https://example.com/a.md)",
    linkBase: "/repo/app/",
  });
  expect(screen.getByRole("link", { name: "site" })).toHaveAttribute(
    "href",
    "https://example.com/a.md",
  );
  await fireEvent.click(screen.getByRole("button", { name: "Preview spec v1.md" }));
  await screen.findByText("spec");
  expect(requestedPath(fetch.mock.calls[0])).toBe("/repo/app/docs/spec v1.md");
  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  await waitFor(() => expect(requestedPath(fetch.mock.calls[1])).toBe("/repo/other/notes.md"));
});

test("follows links inside a previewed file and returns to the previous reading position", async () => {
  const files: Record<string, string> = {
    "/repo/docs/index.md":
      "# Index\n\nSee [the guide](guide/setup.md#install-steps) and [top](#index).\n",
    "/repo/docs/guide/setup.md": "# Setup\n\n## Install steps\n\nBack to [index](../index.md).\n",
  };
  const fetch = vi
    .fn()
    .mockImplementation((url: string) =>
      Promise.resolve(new Response(files[requestedPath([url])!])),
    );
  vi.stubGlobal("fetch", fetch);
  const scrollIntoView = vi.fn();
  HTMLElement.prototype.scrollIntoView = scrollIntoView;
  message("/repo/docs/index.md");
  await fireEvent.click(screen.getByRole("button", { name: "Preview index.md" }));
  const panel = screen.getByRole("complementary");
  await within(panel).findByRole("heading", { name: "Index" });
  expect(within(panel).queryByRole("button", { name: "Back" })).not.toBeInTheDocument();

  await fireEvent.click(within(panel).getByRole("button", { name: "Preview index.md" }));
  await waitFor(() => expect(scrollIntoView).toHaveBeenCalled());
  expect(fetch).toHaveBeenCalledTimes(1);

  const content = panel.querySelector<HTMLElement>("[data-file-preview-content]")!;
  content.scrollTop = 120;
  scrollIntoView.mockClear();
  await fireEvent.click(within(panel).getByRole("button", { name: "Preview setup.md" }));
  const target = await within(panel).findByRole("heading", { name: "Install steps" });
  await waitFor(() => expect(scrollIntoView.mock.contexts).toContain(target));
  expect(
    within(panel).getByText("/repo/docs/guide/setup.md", { selector: "p" }),
  ).toBeInTheDocument();

  const scrollTo = vi.fn();
  HTMLElement.prototype.scrollTo = scrollTo;
  await fireEvent.click(within(panel).getByRole("button", { name: "Back" }));
  await within(panel).findByRole("heading", { name: "Index" });
  await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ top: 120 }));
  expect(within(panel).queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
  expect(fetch).toHaveBeenCalledTimes(3);
});

test("opening a link from chat starts a new preview history", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(() => Promise.resolve(new Response("[next](next.md)"))),
  );
  message("/repo/a.md");
  await fireEvent.click(screen.getByRole("button", { name: "Preview a.md" }));
  const panel = screen.getByRole("complementary");
  await fireEvent.click(await within(panel).findByRole("button", { name: "Preview next.md" }));
  await within(panel).findByRole("button", { name: "Back" });
  const chatLink = screen
    .getAllByRole("button", { name: "Preview a.md" })
    .find((button) => !panel.contains(button))!;
  await fireEvent.click(chatLink);
  const reopened = screen.getByRole("complementary");
  await within(reopened).findByRole("button", { name: "Preview next.md" });
  expect(within(reopened).queryByRole("button", { name: "Back" })).not.toBeInTheDocument();
});

test("opens local image links in the image viewer instead of the preview panel", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(new Response("png", { headers: { "Content-Type": "image/png" } }))
    .mockResolvedValueOnce(
      new Response("text", { headers: { "Content-Type": "application/octet-stream" } }),
    );
  vi.stubGlobal("fetch", fetch);
  render(FilePreviewHarness, {
    content: "![chart](img/chart.png) [fake](img/fake.png)",
    linkBase: "/repo/",
  });
  expect(document.querySelector("img")).toBeNull();

  await fireEvent.click(screen.getByRole("button", { name: "Enlarge chart.png" }));
  const viewer = await screen.findByRole("dialog");
  await waitFor(() =>
    expect(within(viewer).getByAltText("chart.png")).toHaveAttribute("src", "blob:download"),
  );
  expect(requestedPath(fetch.mock.calls[0])).toBe("/repo/img/chart.png");
  expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
  await fireEvent.keyDown(viewer, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

  await fireEvent.click(screen.getByRole("button", { name: "Enlarge fake.png" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("not a PNG, JPEG, GIF, or WebP image");
  expect(screen.getByRole("button", { name: "Download" })).toBeEnabled();
});
