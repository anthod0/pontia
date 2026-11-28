import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import { languagePreference } from "../../src/lib/i18n";
import type { Bot } from "../../src/api/types";
import BotSettingsForm from "../../src/components/bots/BotSettingsForm.svelte";

const bot: Bot = {
  bot_id: "bot-one",
  name: "Bot",
  description: null,
  client_type: "pi",
  client_key: "default",
  model: "provider/first",
  reasoning_effort: null,
  avatar_shape: "classic",
  avatar_color: "blue",
};
const high = { id: "high", name: "high", description: "" };
const models = [
  { id: "provider/first", name: "First model", description: "provider" },
  { id: "provider/second", name: "Second model", description: "provider" },
];

beforeEach(() => {
  vi.spyOn(testDevice.api, "listBotClients").mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", client_key: "default", version: "0.70.2" },
    {
      id: "another",
      name: "Another client",
      client_type: "another",
      client_key: "custom",
      version: null,
    },
  ]);
  vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models,
    version: "0.70.2",
    fetched_at: Date.now(),
    default_model: null,
  });
});
afterEach(() => vi.restoreAllMocks());

test("searches the catalog and saves the selected model ID when editing a Bot", async () => {
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  await fireEvent.click(screen.getByLabelText("Model"));
  await fireEvent.input(screen.getByLabelText("Search models"), {
    target: { value: "provider/second" },
  });
  expect(screen.queryByLabelText("First model")).not.toBeInTheDocument();
  await fireEvent.click(screen.getByLabelText("Second model"));
  await fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith(
      {
        avatar_shape: "classic",
        avatar_color: "blue",
        name: "Bot",
        description: null,
        client_type: "pi",
        client_key: "default",
        model: "provider/second",
        reasoning_effort: null,
      },
      "bot-one",
    ),
  );
});

test("requires an explicit choice when a saved effort is missing from the catalog", async () => {
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  vi.mocked(testDevice.api.listAgentClientModels).mockResolvedValue({
    models: [{ ...models[0], reasoning_efforts: [high] }, models[1]],
    version: "0.70.2",
    fetched_at: Date.now(),
    default_model: null,
  });
  render(BotSettingsForm, { bot: { ...bot, reasoning_effort: "removed" }, onsaved: vi.fn() });
  await screen.findByRole("alert");
  const submit = screen.getByRole("button", { name: "Save" });
  expect(submit).toBeDisabled();
  const trigger = screen.getByLabelText("Model");
  expect(trigger).toHaveAttribute("aria-invalid", "true");
  expect(trigger).toHaveTextContent("removed");
  await fireEvent.click(trigger);
  const efforts = await screen.findByLabelText("Reasoning effort");
  await fireEvent.click(within(efforts).getByLabelText("Default"));
  await waitFor(() => expect(submit).toBeEnabled());
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ reasoning_effort: null }),
      "bot-one",
    ),
  );
});

test("switching clients clears the selection and loads that client catalog", async () => {
  vi.mocked(testDevice.api.listAgentClientModels).mockImplementation(async (client) =>
    client === "pi"
      ? { models, version: "0.70.2", fetched_at: Date.now(), default_model: null }
      : {
          models: [{ id: "other-model", name: "Other model", description: "" }],
          version: null,
          fetched_at: Date.now(),
          default_model: null,
        },
  );
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  const submit = screen.getByRole("button", { name: "Save" });
  await waitFor(() => expect(submit).toBeEnabled());
  const client = screen.getByLabelText("Agent client");
  expect(client).toHaveTextContent(/^Pi$/);
  await fireEvent.keyDown(client, { key: "ArrowDown" });
  await screen.findByRole("option", { name: "Another client" });
  await fireEvent.keyDown(client, { key: "End" });
  await fireEvent.keyDown(client, { key: "Enter" });
  await waitFor(() => expect(submit).toBeDisabled());
  expect(save).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByLabelText("Model"));
  await fireEvent.click(await screen.findByLabelText("Other model"));
  await waitFor(() => expect(submit).toBeEnabled());
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith(
      {
        name: "Bot",
        description: null,
        client_type: "another",
        client_key: "custom",
        model: "other-model",
        reasoning_effort: null,
        avatar_shape: "classic",
        avatar_color: "blue",
      },
      "bot-one",
    ),
  );
});

test("opens the client picker without submitting the Bot form on mobile", async () => {
  const matchMedia = window.matchMedia;
  window.matchMedia = ((query: string) => ({
    ...matchMedia(query),
    matches: query.includes("max-width"),
  })) as typeof matchMedia;
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  const onsaved = vi.fn();

  try {
    render(BotSettingsForm, { bot, onsaved });
    const trigger = await screen.findByLabelText("Agent client");
    expect(trigger).toHaveAttribute("aria-haspopup", "dialog");
    await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
    await fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(await screen.findByRole("group", { name: "Agent client" })).toBeVisible();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.getByRole("group", { name: "Agent client" })).toBeVisible();
    expect(save).not.toHaveBeenCalled();
    expect(onsaved).not.toHaveBeenCalled();
  } finally {
    window.matchMedia = matchMedia;
  }
});

test("retries a failed catalog request and requires an available model before saving", async () => {
  vi.mocked(testDevice.api.listAgentClientModels)
    .mockRejectedValueOnce(new Error("Offline"))
    .mockResolvedValueOnce({
      models,
      version: "0.70.2",
      fetched_at: Date.now(),
      default_model: null,
    });
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
});

test.each([{ catalog: [] }, { catalog: models.slice(1) }])(
  "does not save an unavailable model",
  async ({ catalog }) => {
    vi.mocked(testDevice.api.listAgentClientModels).mockResolvedValue({
      models: catalog,
      version: "0.70.2",
      fetched_at: Date.now(),
      default_model: catalog[0]?.id ?? null,
    });
    render(BotSettingsForm, { bot, onsaved: vi.fn() });
    await waitFor(() =>
      expect(
        testDevice.queryClient.getQueryData(["agent-client-models", "pi", "default", "0.70.2"]),
      ).toBeDefined(),
    );
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  },
);

test("preselects the client default model for a new Bot in the selected language", async ({
  onTestFinished,
}) => {
  onTestFinished(() => languagePreference.set("system"));
  languagePreference.set("zh-CN");
  vi.mocked(testDevice.api.listAgentClientModels).mockResolvedValue({
    models,
    version: "0.70.2",
    fetched_at: Date.now(),
    default_model: "provider/second",
  });
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  render(BotSettingsForm, { onsaved: vi.fn() });
  await fireEvent.input(screen.getByLabelText("名称"), { target: { value: "New" } });
  await waitFor(() => expect(screen.getByLabelText("模型")).toHaveTextContent("Second model"));
  await fireEvent.click(screen.getByRole("button", { name: "创建 Bot" }));
  await waitFor(() =>
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ client_type: "pi", model: "provider/second" }),
      undefined,
      "zh-CN",
    ),
  );
});

test("keeps the saved model usable while a client upgrade refreshes its catalog", async () => {
  const saved = { models, default_model: null, version: "0.70.2", fetched_at: Date.now() };
  const api = vi.mocked(testDevice.api.listAgentClientModels).mockResolvedValue(saved);
  const first = render(BotSettingsForm, { bot, onsaved: vi.fn() });
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  first.unmount();
  testDevice.queryClient.clear();
  vi.mocked(testDevice.api.listBotClients).mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", client_key: "default", version: "0.71" },
  ]);
  const pending = Promise.withResolvers<typeof saved>();
  api.mockResolvedValueOnce(saved).mockReturnValueOnce(pending.promise);
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  await waitFor(() => expect(screen.getByLabelText("Model")).toHaveTextContent("First model"));
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  pending.resolve({
    ...saved,
    version: "0.71",
    fetched_at: Date.now(),
    models: [{ ...models[0], name: "Renamed model" }],
  });
  await waitFor(() => expect(screen.getByLabelText("Model")).toHaveTextContent("Renamed model"));
});
