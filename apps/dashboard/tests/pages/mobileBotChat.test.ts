import "../shallowRouting";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, onTestFinished, test, vi } from "vitest";
import type { AgentClientModels, BotMessagePage } from "../../src/api/types";
import { createSampleData } from "../../mock/data";
import { languagePreference, translate } from "../../src/lib/i18n";
import { testDevice } from "../deviceFixture";
import MobileBotChatHarness from "./MobileBotChatHarness.svelte";

afterEach(() => vi.restoreAllMocks());

const id = "bmsg_01234567-89ab-7abc-8def-000000000001";
const message = {
  id,
  kind: "bot_reply",
  status: "normal",
  created_at: "2026-10-01T01:00:00Z",
  content: "Review this",
  todo: { state: "open", by: "bot", at: "2026-11-21T08:00:00Z" },
} as const;
const data: BotMessagePage = {
  bot: {
    bot_id: "review",
    name: "Reviewer",
    description: null,
    avatar_shape: "classic",
    avatar_color: "blue",
    client_type: "pi",
    client_key: "default",
    model: "test",
    reasoning_effort: null,
  },
  entries: [message],
  open_todo_count: 1,
  file_link_session: null,
  notice_session_id: null,
  generation_notice: null,
  next_before: null,
  status: { kind: "idle" },
};

function openChat(botId = "review") {
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue(data);
  vi.spyOn(testDevice.access, "uploadStorageScope").mockResolvedValue("mobile-draft");
  return render(MobileBotChatHarness, { botId });
}

test("selecting /reload from the mobile command menu reloads the Bot and clears the draft", async () => {
  const anchor = new DOMRect(56, 760, 278, 44);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue(anchor);
  vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue([
    anchor,
  ] as unknown as DOMRectList);
  vi.spyOn(document.documentElement, "clientWidth", "get").mockReturnValue(390);
  vi.spyOn(document.documentElement, "clientHeight", "get").mockReturnValue(844);
  const reload = vi.spyOn(testDevice.api, "reloadBot").mockResolvedValue();
  openChat();
  const input = await screen.findByRole("textbox", { name: "Message" });
  await userEvent.type(input, "/");
  await userEvent.click(await screen.findByRole("option", { name: /\/reload/ }));
  await waitFor(() => expect(reload).toHaveBeenCalledExactlyOnceWith("review"));
  await waitFor(() => expect(input.textContent).toBe(""));
});

test("quoting a todo returns to the current Bot draft and sends both together", async () => {
  vi.spyOn(testDevice.api, "getBotTodos").mockResolvedValue({
    open: [message],
    recently_resolved: [],
  });
  const send = vi
    .spyOn(testDevice.api, "sendBotMessage")
    .mockResolvedValue({ ...message, id: "new", kind: "user_input" });
  openChat();
  const input = await screen.findByRole("textbox", { name: "Message" });
  await userEvent.type(input, "Looks good");
  await fireEvent.click(screen.getByRole("button", { name: "Todo · 1" }));
  await fireEvent.click(
    await within(await screen.findByRole("dialog")).findByRole("button", { name: "Quote" }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(within(input).getByRole("button", { name: /^Remove quote/ })).toBeVisible();
  expect(input).toHaveTextContent("Looks good");
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" })).toBeEnabled());
  await fireEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() =>
    expect(send).toHaveBeenCalledWith(
      "review",
      `<user_quote message_id="${id}" speaker="Bot">Review this</user_quote>\nLooks good`,
    ),
  );
});

function emptyProfiles() {
  vi.spyOn(testDevice.api, "listAgentProfiles").mockResolvedValue([]);
  vi.spyOn(testDevice.api, "listAgentClients").mockResolvedValue(createSampleData().agentClients);
}

const hint = () => screen.findByRole("region", { name: /specialist helpers/ });

test.each([
  ["Hide in Reviewer only", true],
  ["Don't show again", false],
])("remembers %s when reopening this Bot or another Bot", async (action, visibleElsewhere) => {
  emptyProfiles();
  const first = openChat();
  await fireEvent.click(within(await hint()).getByRole("button", { name: action }));
  first.unmount();
  const reopened = openChat();
  await screen.findByRole("textbox");
  expect(screen.queryByRole("region", { name: /specialist helpers/ })).toBeNull();
  reopened.unmount();
  openChat("other");
  await screen.findByRole("textbox");
  expect(!!screen.queryByRole("region", { name: /specialist helpers/ })).toBe(visibleElsewhere);
});

test.each(["zh-CN", "en"] as const)(
  "asking in %s retries the setup request and keeps the existing draft",
  async (locale) => {
    onTestFinished(() => languagePreference.set("system"));
    languagePreference.set(locale);
    emptyProfiles();
    const send = vi
      .spyOn(testDevice.api, "sendBotMessage")
      .mockRejectedValueOnce(new Error("Offline"))
      .mockResolvedValue({ ...message, id: "sent", kind: "user_input" });
    openChat();
    const input = await screen.findByRole("textbox");
    await userEvent.type(input, "My unfinished message");
    const ask = await screen.findByRole("button", {
      name: translate("bots.profileHint.askBot", { name: "Reviewer" }),
    });
    await fireEvent.click(ask);
    await waitFor(() => expect(ask).toBeEnabled());
    await fireEvent.click(ask);
    await waitFor(() => expect(ask).not.toBeInTheDocument());
    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenLastCalledWith("review", translate("bots.profileHint.askMessage"));
    expect(send.mock.calls[1][1]).toContain("pontia bot profile create -h");
    expect(input).toHaveTextContent("My unfinished message");
  },
);

test("quick setup offers only enabled templates and hides when the first profile arrives", async () => {
  const sample = createSampleData();
  const clients = sample.agentClients.filter((client) => client.client_type === "codex");
  vi.spyOn(testDevice.api, "listAgentClients").mockResolvedValue(clients);
  const profiles = vi.spyOn(testDevice.api, "listAgentProfiles").mockResolvedValue([]);
  openChat();
  const region = await hint();
  expect(within(region).getByRole("button", { name: /Coder/ })).toBeEnabled();
  expect(within(region).queryByRole("button", { name: /Researcher/ })).toBeNull();
  profiles.mockResolvedValue(sample.agentProfiles);
  await testDevice.queryClient.invalidateQueries({
    queryKey: testDevice.agentProfileQueries.agentProfileKeys.all,
  });
  await waitFor(() => expect(region).not.toBeInTheDocument());
});

test("saves edits made while resolving a template's model and while the Bot becomes busy", async () => {
  const sample = createSampleData();
  const client = sample.agentClients.find((client) => client.client_type === "codex")!;
  vi.spyOn(testDevice.api, "listAgentClients").mockResolvedValue([client]);
  vi.spyOn(testDevice.api, "listAgentProfiles").mockResolvedValue([]);
  let resolveCatalog!: (catalog: AgentClientModels) => void;
  const catalog = new Promise<AgentClientModels>((resolve) => {
    resolveCatalog = resolve;
  });
  vi.spyOn(testDevice.api, "listAgentClientModels").mockReturnValue(catalog);
  const createProfile = vi
    .spyOn(testDevice.api, "createAgentProfile")
    .mockImplementation(async (input) => ({
      ...sample.agentProfiles[0],
      ...input,
    }));
  openChat();
  await fireEvent.click(within(await hint()).getByRole("button", { name: /Coder/ }));
  const dialog = within(await screen.findByRole("dialog"));
  const name = dialog.getByRole("textbox", { name: "Name", exact: true });
  expect(name).toHaveValue("Coder");
  await fireEvent.input(name, { target: { value: "Release Engineer" } });
  vi.mocked(testDevice.api.getBotMessages).mockResolvedValue({
    ...data,
    status: { kind: "processing" },
  });
  window.dispatchEvent(new Event("online"));
  await screen.findByText(translate("bots.processing"));
  resolveCatalog({
    version: client.version,
    fetched_at: Date.now(),
    models: [{ id: "local", name: "Local", description: "" }],
    default_model: "local",
  });
  const add = dialog.getByRole("button", { name: "Add", exact: true });
  await waitFor(() => expect(add).not.toHaveAttribute("aria-disabled", "true"));
  await fireEvent.click(add);
  await waitFor(() =>
    expect(createProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        profile_id: "coder",
        name: "Release Engineer",
        client_type: "codex",
        model: "local",
      }),
    ),
  );
});

test("offers setup and the full catalog when an enabled client has no quick template", async () => {
  const client = createSampleData().agentClients.find((client) => client.client_type === "codex")!;
  vi.spyOn(testDevice.api, "listAgentClients").mockResolvedValue([
    { ...client, client_type: "acp", client_key: "custom" },
  ]);
  vi.spyOn(testDevice.api, "listAgentProfiles").mockResolvedValue([]);
  openChat();
  const region = within(await hint());
  expect(region.getByRole("button", { name: "All templates" })).toBeEnabled();
  expect(region.getByRole("button", { name: /Ask Reviewer/ })).toBeEnabled();
  expect(region.queryByRole("button", { name: /Coder/ })).toBeNull();
});
