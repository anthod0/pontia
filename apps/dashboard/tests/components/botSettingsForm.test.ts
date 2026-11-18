import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot } from "../../src/api/types";
import BotSettingsForm from "../../src/components/bots/BotSettingsForm.svelte";

const bot: Bot = {
  bot_id: "bot-one",
  name: "Bot",
  description: null,
  client_type: "pi",
  client_key: "default",
  model: "provider/first",
  working_on: null,
  avatar_shape: "classic",
  avatar_color: "blue",
};
const models = [
  { id: "provider/first", name: "First model", description: "provider" },
  { id: "provider/second", name: "Second model", description: "provider" },
];

beforeEach(() => {
  vi.spyOn(testDevice.api, "listBotClients").mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", client_key: "default" },
    { id: "another", name: "Another client", client_type: "another", client_key: "custom" },
  ]);
  vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue(models);
});
afterEach(() => vi.restoreAllMocks());

test("searches the catalog and saves the selected model ID when editing a Bot", async () => {
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
  await fireEvent.click(screen.getByLabelText("Model"));
  await fireEvent.input(screen.getByRole("textbox", { name: "Search models" }), {
    target: { value: "provider/second" },
  });
  expect(screen.queryByRole("button", { name: "First model" })).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Second model" }));
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
      },
      "bot-one",
    ),
  );
});

test("switching clients clears the selection and loads that client catalog", async () => {
  vi.mocked(testDevice.api.listAgentClientModels).mockImplementation(async (client) =>
    client === "pi" ? models : [{ id: "other-model", name: "Other model", description: "" }],
  );
  const save = vi.spyOn(testDevice.api, "saveBot").mockResolvedValue(bot);
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  const submit = screen.getByRole("button", { name: "Save" });
  await waitFor(() => expect(submit).toBeEnabled());
  const client = screen.getByLabelText("Agent client");
  await fireEvent.keyDown(client, { key: "ArrowDown" });
  await screen.findByRole("option", { name: "Another client" });
  await fireEvent.keyDown(client, { key: "End" });
  await fireEvent.keyDown(client, { key: "Enter" });
  await waitFor(() => expect(submit).toBeDisabled());
  await fireEvent.click(screen.getByLabelText("Model"));
  await fireEvent.click(await screen.findByRole("button", { name: "Other model" }));
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
        avatar_shape: "classic",
        avatar_color: "blue",
      },
      "bot-one",
    ),
  );
});

test("retries a failed catalog request and requires an available model before saving", async () => {
  vi.mocked(testDevice.api.listAgentClientModels)
    .mockRejectedValueOnce(new Error("Offline"))
    .mockResolvedValueOnce(models);
  render(BotSettingsForm, { bot, onsaved: vi.fn() });
  await screen.findByRole("alert");
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Save" })).toBeEnabled());
});

test.each([{ catalog: [] }, { catalog: models.slice(1) }])(
  "does not save an unavailable model",
  async ({ catalog }) => {
    vi.mocked(testDevice.api.listAgentClientModels).mockResolvedValue(catalog);
    render(BotSettingsForm, { bot, onsaved: vi.fn() });
    await waitFor(() =>
      expect(testDevice.queryClient.getQueryData(["agent-client-models", "pi", "default"])).toEqual(
        catalog,
      ),
    );
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  },
);
