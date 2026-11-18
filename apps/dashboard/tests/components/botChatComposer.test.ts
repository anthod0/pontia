import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import { loadUploadRecords, saveUploadRecord } from "../../src/lib/uploads/records";

afterEach(() => vi.restoreAllMocks());

async function openChat() {
  const botId = crypto.randomUUID();
  const scope = `bot-test:${botId}`;
  const draft = `bot:${botId}`;
  vi.spyOn(testDevice.access, "uploadStorageScope").mockResolvedValue(scope);
  vi.spyOn(testDevice.api, "getBotChat").mockResolvedValue({
    bot: {
      bot_id: botId,
      name: "Bot",
      description: null,
      client_type: "pi",
      client_key: "default",
      model: "test",
      working_on: null,
    },
    messages: [],
    notices: [],
  });
  await saveUploadRecord({
    scope,
    draft,
    key: crypto.randomUUID(),
    uploadId: "upload-1",
    name: "report.txt",
    size: 3,
    sha256: "a".repeat(64),
  });
  vi.spyOn(testDevice.uploads, "uploadRequest").mockResolvedValue({
    upload_id: "upload-1",
    name: "report.txt",
    size: 3,
    sha256: "a".repeat(64),
    offset: 3,
    completed: true,
    path: "/device/report.txt",
    created_at: "2026-11-18T00:00:00Z",
  });
  const send = vi.spyOn(testDevice.api, "sendBotMessage");
  render(BotChatPage, { botId });
  const submit = await screen.findByRole("button", { name: "Send" });
  await waitFor(() => expect(submit).toBeEnabled());
  return { botId, scope, draft, send, submit };
}

const attachment =
  '<user_uploads note="Temporary files; copy any you need into the workspace.">\n<user_upload path="/device/report.txt">report.txt</user_upload>\n</user_uploads>';

test("sends a file without text to the Bot and clears the attachment draft", async () => {
  const { botId, scope, draft, send, submit } = await openChat();
  send.mockResolvedValue();
  await fireEvent.click(submit);
  await waitFor(() => expect(send).toHaveBeenCalledExactlyOnceWith(botId, attachment));
  await waitFor(async () => expect(await loadUploadRecords(scope, draft)).toEqual([]));
  expect(submit).toBeDisabled();
});

test("keeps text and attachments after a rejected send so the user can retry", async () => {
  const { botId, scope, draft, send, submit } = await openChat();
  send.mockRejectedValueOnce(new Error("Offline")).mockResolvedValueOnce();
  const editor = screen.getByRole("textbox");
  await userEvent.type(editor, "Read this report");
  await fireEvent.click(submit);
  await screen.findByRole("alert");
  await waitFor(() => expect(submit).toBeEnabled());
  expect(editor).toHaveTextContent("Read this report");
  expect(editor).not.toHaveTextContent("user_upload");
  expect(await loadUploadRecords(scope, draft)).toHaveLength(1);
  await fireEvent.click(submit);
  await waitFor(async () => expect(await loadUploadRecords(scope, draft)).toEqual([]));
  expect(send).toHaveBeenNthCalledWith(2, botId, `${attachment}\nRead this report`);
  expect(editor.textContent).toBe("");
});
