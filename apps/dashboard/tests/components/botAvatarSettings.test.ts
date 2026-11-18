import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { Bot } from "../../src/api/types";
import BotSettingsForm from "../../src/components/bots/BotSettingsForm.svelte";

afterEach(() => vi.restoreAllMocks());

const bot: Bot = {
  bot_id: "avatar-test",
  name: "Bot",
  description: null,
  avatar_shape: "cloud",
  avatar_color: "aqua",
  client_type: "pi",
  client_key: "default",
  model: "provider/model",
  working_on: null,
};

test("edits an avatar, retains the selection after a failed save, and saves it on retry", async () => {
  vi.spyOn(testDevice.api, "listBotClients").mockResolvedValue([
    { id: "pi", name: "Pi", client_type: "pi", client_key: "default" },
  ]);
  vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue({
    models: [{ id: "provider/model", name: "Test model" }],
    default_model: null,
  });
  const saved = { ...bot, avatar_shape: "onigiri", avatar_color: "indigo" } as const;
  const save = vi
    .spyOn(testDevice.api, "saveBot")
    .mockRejectedValueOnce(new Error("Offline"))
    .mockResolvedValue(saved);
  const onsaved = vi.fn();
  render(BotSettingsForm, { bot, onsaved });
  expect(screen.getByRole("radio", { name: "Cloud" })).toBeChecked();
  expect(screen.getByRole("radio", { name: "Aqua" })).toBeChecked();
  await fireEvent.click(screen.getByRole("radio", { name: "Onigiri" }));
  await fireEvent.click(screen.getByRole("radio", { name: "Indigo" }));
  await fireEvent.click(screen.getByRole("radio", { name: "Indigo" }));
  const submit = screen.getByRole("button", { name: "Save" });
  expect(screen.getByRole("radio", { name: "Indigo" })).toBeChecked();
  await waitFor(() => expect(submit).toBeEnabled());
  await fireEvent.click(submit);
  await screen.findByRole("alert");
  expect(onsaved).not.toHaveBeenCalled();
  await fireEvent.click(submit);
  await waitFor(() => expect(onsaved).toHaveBeenCalledWith(saved));
  expect(save).toHaveBeenLastCalledWith(
    {
      name: "Bot",
      description: null,
      client_type: "pi",
      client_key: "default",
      model: "provider/model",
      avatar_shape: "onigiri",
      avatar_color: "indigo",
    },
    "avatar-test",
  );
});
