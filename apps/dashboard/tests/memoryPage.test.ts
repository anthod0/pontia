import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import MemoryPage from "../src/pages/MemoryPage.svelte";
import { token } from "../src/stores/auth";
import { createEmptyData, createSampleData } from "../mock/data";
import { createMockDevice, handleDeviceRequest, type MockDevice } from "../mock/server";
import { testDevice } from "./deviceFixture";

let device: MockDevice;
function serve(data = createSampleData()) {
  device = createMockDevice(data);
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = new Request(new URL(String(input), "http://localhost"), init);
      return handleDeviceRequest(
        device,
        request,
        new URL(request.url).pathname.replace("/api/v1", ""),
      );
    }),
  );
}
beforeEach(() => {
  token.set("test-token");
  testDevice.queryClient.clear();
  serve();
});
afterEach(() => vi.unstubAllGlobals());

test("lists active memories newest first and searches their text", async () => {
  const user = userEvent.setup();
  render(MemoryPage);
  const list = await screen.findByRole("list", { name: "Memories" });
  const items = within(list).getAllByRole("listitem");
  const active = device.data.memory.memories.filter((memory) => !memory.archived);
  expect(items).toHaveLength(active.length);
  expect(screen.getByText(`${active.length} memories`)).toBeInTheDocument();
  expect(items[0]).toHaveTextContent("Codex");
  expect(within(list).queryByText(/只保存在当前浏览器中/)).toBeNull();

  await user.type(screen.getByRole("textbox", { name: "Search memories" }), "cloudflare");
  expect(await screen.findByText("2 matching memories")).toBeInTheDocument();
  expect(
    within(screen.getByRole("list", { name: "Memories" })).getAllByRole("listitem"),
  ).toHaveLength(2);

  await user.clear(screen.getByRole("textbox", { name: "Search memories" }));
  await user.type(screen.getByRole("textbox", { name: "Search memories" }), "no such words");
  expect(await screen.findByText("No matching memories")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Clear search" }));
  expect(await screen.findByText(`${active.length} memories`)).toBeInTheDocument();
});

test("shows USER.md in the About you tab", async () => {
  const user = userEvent.setup();
  render(MemoryPage);
  await user.click(screen.getByRole("tab", { name: "About you" }));
  expect(await screen.findByText("USER.md")).toBeInTheDocument();
  await user.click(screen.getByRole("radio", { name: "Source" }));
  expect(screen.getByText(/称呼：安东/)).toBeInTheDocument();
});

test("explains where memories and USER.md come from on a new device", async () => {
  serve(createEmptyData());
  const user = userEvent.setup();
  render(MemoryPage);
  expect(await screen.findByText("Memories appear here")).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: "Search memories" })).toBeNull();
  await user.click(screen.getByRole("tab", { name: "About you" }));
  expect(await screen.findByText("Bots fill this in as they learn about you")).toBeInTheDocument();
});

test("edits USER.md and keeps a stale draft from overwriting a Bot's change", async () => {
  const user = userEvent.setup();
  render(MemoryPage);
  await user.click(screen.getByRole("tab", { name: "About you" }));
  await user.click(await screen.findByRole("button", { name: "Edit" }));
  const editor = screen.getByRole("textbox", { name: "USER.md contents" });
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  await user.clear(editor);
  await user.type(editor, "称呼：小林");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText("称呼：小林")).toBeInTheDocument();
  expect(device.data.memory.user.text).toBe("称呼：小林");

  await user.click(screen.getByRole("button", { name: "Edit" }));
  device.data.memory.user = { text: "称呼：林（Bot 改的）", updated_at: new Date().toISOString() };
  await user.type(screen.getByRole("textbox", { name: "USER.md contents" }), "\n语言：中文");
  await user.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText(/A Bot changed USER.md/)).toBeInTheDocument();
  expect(device.data.memory.user.text).toBe("称呼：林（Bot 改的）");
  const startOver = screen.getByRole("button", { name: "Edit the latest version" });
  await waitFor(() => expect(startOver).toBeEnabled());
  await user.click(startOver);
  expect(screen.getByRole("textbox", { name: "USER.md contents" })).toHaveValue(
    "称呼：林（Bot 改的）",
  );
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.getByText("称呼：林（Bot 改的）")).toBeInTheDocument();
});

test("blocks saves over the character limit", async () => {
  serve(createEmptyData());
  const user = userEvent.setup();
  render(MemoryPage);
  await user.click(screen.getByRole("tab", { name: "About you" }));
  await user.click(await screen.findByRole("button", { name: "Write it yourself" }));
  const editor = screen.getByRole("textbox", { name: "USER.md contents" });
  await user.click(editor);
  await user.paste("字".repeat(2001));
  expect(editor).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByText("2,001 / 2,000 characters")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
});
