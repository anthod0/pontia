import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SharedFilePreviewHarness from "./SharedFilePreviewHarness.svelte";
import { get } from "svelte/store";
import { downloads } from "../../src/lib/downloads/manager";
import { testDevice } from "../deviceFixture";
import { token } from "../../src/stores/auth";
import { VisibleObserver } from "./fileCardFixture";

beforeEach(() => {
  token.set("shared-preview-token");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  token.set("");
});

test("an attached text file opens in the file preview panel", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response("# Notes\n"));
  vi.stubGlobal("fetch", fetch);
  render(SharedFilePreviewHarness, { paths: ["/tmp/out/notes.md"] });
  expect(fetch).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole("button", { name: "Preview notes.md" }));
  const panel = screen.getByRole("complementary");
  await within(panel).findByRole("heading", { name: "Notes" });
  const request = new URL(fetch.mock.calls[0][0], "http://localhost");
  expect(request.pathname).toBe("/api/v1/sessions/session-1/files/content");
  expect(request.searchParams.get("path")).toBe("/tmp/out/notes.md");
});

test.each(["demo.html", "demo.HTM"])(
  "opens %s in a sandbox and switches to source",
  async (name) => {
    const html =
      "<img src='https://example.com/logo.png'><button onclick='this.textContent = 42'>Click</button>";
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(html)));
    render(SharedFilePreviewHarness, { paths: [`/tmp/${name}`] });
    await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
    const panel = within(screen.getByRole("complementary"));
    const frame = await panel.findByTitle(`Preview ${name}`);
    expect(frame).toBeInstanceOf(HTMLIFrameElement);
    expect(frame).toHaveAttribute("sandbox", "allow-scripts allow-forms");
    expect(frame).toHaveAttribute("srcdoc", html);

    await fireEvent.click(panel.getByRole("tab", { name: "Source" }));
    expect(frame).not.toBeInTheDocument();
    expect(panel.getByRole("tabpanel", { name: "Source" }).textContent).toBe(html);
    await fireEvent.click(panel.getByRole("tab", { name: "Preview" }));
    expect(await panel.findByTitle(`Preview ${name}`)).toHaveAttribute("srcdoc", html);
  },
);

test("a card reads each file's details with a HEAD request on the path route", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  const fetch = vi.fn().mockResolvedValue(
    new Response(null, {
      headers: { "Content-Length": "2048", "Content-Type": "application/pdf" },
    }),
  );
  vi.stubGlobal("fetch", fetch);
  render(SharedFilePreviewHarness, { paths: ["/tmp/out/report.pdf"] });
  expect(await screen.findByText("PDF · 2 KiB")).toBeInTheDocument();
  const [url, init] = fetch.mock.calls[0];
  expect(init.method).toBe("HEAD");
  expect(new URL(url, "http://localhost").searchParams.get("path")).toBe("/tmp/out/report.pdf");
});

test("an attached file the preview panel cannot show has no preview action", () => {
  render(SharedFilePreviewHarness, { paths: ["/tmp/out/demo.mp4"] });
  expect(screen.queryByRole("button", { name: "Preview demo.mp4" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Download demo.mp4" })).toBeInTheDocument();
});

test("a missing attached file loses its download and preview actions", async () => {
  vi.stubGlobal("IntersectionObserver", VisibleObserver);
  const unregister = downloads.registerDevice(testDevice);
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(new Response(null, { status: 404, statusText: "Not Found" })),
  );
  try {
    render(SharedFilePreviewHarness, { paths: ["/tmp/missing.txt"] });
    expect(await screen.findByRole("alert")).toHaveTextContent("File no longer exists");
    expect(screen.queryByRole("button", { name: "Preview missing.txt" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Download missing.txt" })).not.toBeInTheDocument();
  } finally {
    for (const task of get(downloads)) downloads.cancel(task.id);
    unregister();
  }
});

test.each([
  ["report.CSV", ","],
  ["report.tsv", "\t"],
])(
  "%s opens as a table, cycles sort order and exposes the original source",
  async (name, delimiter) => {
    const source = [
      ["Item", "Value"],
      ["Item10", "2.5"],
      ["Item2", "-1"],
      ["Item1", "10"],
    ]
      .map((row) => row.join(delimiter))
      .join("\n");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(source)));
    render(SharedFilePreviewHarness, { paths: [`/tmp/${name}`] });
    await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
    const table = within(await screen.findByRole("table"));
    const items = () =>
      table
        .getAllByRole("row")
        .slice(1)
        .map((row) => within(row).getAllByRole("cell")[0].textContent);
    const value = table.getByRole("button", { name: "Value" });
    await fireEvent.click(value);
    expect(items()).toEqual(["Item2", "Item10", "Item1"]);
    await fireEvent.click(value);
    expect(items()).toEqual(["Item1", "Item10", "Item2"]);
    await fireEvent.click(value);
    expect(items()).toEqual(["Item10", "Item2", "Item1"]);
    await fireEvent.click(table.getByRole("button", { name: "Item" }));
    expect(items()).toEqual(["Item1", "Item2", "Item10"]);
    await fireEvent.click(screen.getByRole("tab", { name: "Source" }));
    expect(screen.getByRole("tabpanel", { name: "Source" }).textContent).toBe(source);
    await fireEvent.click(screen.getByRole("tab", { name: "Preview" }));
    expect(await screen.findByRole("table")).toBeVisible();
  },
);

test.each([
  ["nested.JSON", '{"settings":{"ports":[8080]},"active":true}'],
  ["nested.yaml", "settings:\n  ports: [8080]\nactive: true"],
  ["nested.yml", "settings: {ports: [8080]}\nactive: true"],
])("browses %s as a tree and switches to its original source", async (name, source) => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(source)));
  render(SharedFilePreviewHarness, { paths: [`/tmp/${name}`] });
  await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
  const panel = within(screen.getByRole("complementary"));
  const settings = await panel.findByRole("button", { name: /settings.*\{1\}/ });
  expect(settings).toHaveAttribute("aria-expanded", "false");
  await fireEvent.click(settings);
  expect(panel.getByRole("button", { name: /ports.*\[1\]/ })).toBeVisible();
  await fireEvent.click(settings);
  expect(panel.queryByRole("button", { name: /ports/ })).toBeNull();
  await fireEvent.click(panel.getByRole("button", { name: "Expand all" }));
  expect(panel.getByText("8080")).toBeVisible();
  await fireEvent.click(panel.getByRole("button", { name: "Collapse all" }));
  expect(panel.queryByText("8080")).toBeNull();
  expect(panel.queryByRole("button", { name: /settings/ })).toBeNull();
  await fireEvent.click(panel.getByRole("tab", { name: "Source" }));
  expect(panel.getByRole("tabpanel", { name: "Source" }).textContent).toBe(source);
  await fireEvent.click(panel.getByRole("tab", { name: "Preview" }));
  expect(panel.getByRole("button", { name: "Expand all" })).toBeVisible();
});

test.each([
  ["empty.json", ""],
  ["bad.json", "{missing: true}"],
  ["bad.yml", "broken: ["],
])("keeps the source accessible when %s cannot be parsed", async (name, source) => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(source)));
  render(SharedFilePreviewHarness, { paths: [`/tmp/${name}`] });
  await fireEvent.click(screen.getByRole("button", { name: `Preview ${name}` }));
  const panel = within(screen.getByRole("complementary"));
  expect(await panel.findByRole("alert")).toBeVisible();
  await fireEvent.click(panel.getByRole("tab", { name: "Source" }));
  expect(panel.getByRole("tabpanel", { name: "Source" }).textContent).toBe(source);
});
