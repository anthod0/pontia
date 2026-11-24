import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vitest";
import { testDevice } from "../deviceFixture";
import type { BotEntry } from "../../src/api/types";
import { ApiError } from "../../src/api/errors";
import BotChatPage from "../../src/pages/BotChatPage.svelte";
import { loadUploadRecords, saveUploadRecord } from "../../src/lib/uploads/records";

const toast = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
  warning: vi.fn(),
  dismiss: vi.fn(),
}));
vi.mock("svelte-sonner", () => ({ toast }));

afterEach(() => {
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

type ToastAction = { label: string; onClick: (event: MouseEvent) => void };
function lastToastAction(): ToastAction {
  return (toast.error.mock.lastCall![1] as { action: ToastAction }).action;
}

async function openChat(entries: BotEntry[] = []) {
  const botId = crypto.randomUUID();
  const scope = `bot-test:${botId}`;
  const draft = `bot:${botId}`;
  vi.spyOn(testDevice.access, "uploadStorageScope").mockResolvedValue(scope);
  vi.spyOn(testDevice.api, "getBotMessages").mockResolvedValue({
    bot: {
      bot_id: botId,
      name: "Bot",
      description: null,
      avatar_shape: "classic",
      avatar_color: "blue",
      client_type: "pi",
      client_key: "default",
      model: "test",
      reasoning_effort: null,
    },
    entries,
    next_before: null,
    file_link_session: null,
    open_todo_count: 0,
    notice_session_id: null,
    generation_notice: null,
    status: { kind: "idle" },
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
  send.mockResolvedValue({
    id: "m1",
    status: "normal",
    kind: "user_input",
    content: attachment,
    created_at: "2026-11-18T00:00:01Z",
  });
  await fireEvent.click(submit);
  await waitFor(() => expect(send).toHaveBeenCalledExactlyOnceWith(botId, attachment));
  await waitFor(async () => expect(await loadUploadRecords(scope, draft)).toEqual([]));
  expect(submit).toBeDisabled();
});

test("acknowledges queued Bot input without offering a failed-send retry", async () => {
  const { send, submit } = await openChat();
  send.mockResolvedValue({
    id: "queued",
    status: "normal",
    delivery: "queued",
    kind: "user_input",
    content: attachment,
    created_at: "2026-11-18T00:00:01Z",
  });
  await fireEvent.click(submit);
  await waitFor(() => expect(toast.info).toHaveBeenCalledOnce());
  expect(toast.error).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
});

test("keeps text and attachments after a rejected send so the user can retry", async () => {
  const { botId, scope, draft, send, submit } = await openChat();
  send.mockRejectedValueOnce(new Error("Offline")).mockResolvedValueOnce({
    id: "m2",
    status: "normal",
    kind: "user_input",
    content: attachment,
    created_at: "2026-11-18T00:00:02Z",
  });
  const editor = screen.getByRole("textbox");
  await userEvent.type(editor, "Read this report");
  await fireEvent.click(submit);
  await waitFor(() => expect(submit).toBeEnabled());
  expect(editor).toHaveTextContent("Read this report");
  expect(editor).not.toHaveTextContent("user_upload");
  expect(await loadUploadRecords(scope, draft)).toHaveLength(1);
  await fireEvent.click(submit);
  await waitFor(async () => expect(await loadUploadRecords(scope, draft)).toEqual([]));
  expect(send).toHaveBeenNthCalledWith(2, botId, `${attachment}\nRead this report`);
  expect(editor.textContent).toBe("");
});

test("reports a rejected send in a toast whose Retry action sends the kept draft again", async () => {
  const { botId, send, submit } = await openChat();
  send.mockRejectedValueOnce(new Error("Offline")).mockResolvedValueOnce({
    id: "m3",
    status: "normal",
    kind: "user_input",
    content: attachment,
    created_at: "2026-11-18T00:00:03Z",
  });
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(toast.error).toHaveBeenCalledExactlyOnceWith(
      "Offline",
      expect.objectContaining({ id: "bot-action-error" }),
    ),
  );
  expect(screen.queryByText("Offline")).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await waitFor(() => expect(submit).toBeEnabled());
  const action = lastToastAction();
  expect(action.label).toBe("Retry");
  action.onClick(new MouseEvent("click"));
  await waitFor(() => expect(send).toHaveBeenNthCalledWith(2, botId, attachment));
});

test("reports a failed message retry in a toast with a Retry action", async () => {
  const failed: BotEntry = {
    id: "m4",
    status: "failed",
    kind: "user_input",
    content: "Hello",
    created_at: "2026-11-18T00:00:04Z",
  };
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(56);
  vi.spyOn(Element.prototype, "clientHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("role") === "log" ? 640 : 56;
  });
  const { botId } = await openChat([failed]);
  const retry = vi
    .spyOn(testDevice.api, "retryBotMessage")
    .mockRejectedValueOnce(new Error("Still offline"))
    .mockResolvedValueOnce({ ...failed, status: "normal" });
  await fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
  await waitFor(() =>
    expect(toast.error).toHaveBeenCalledExactlyOnceWith(
      "Still offline",
      expect.objectContaining({ id: "bot-action-error" }),
    ),
  );
  expect(screen.queryByText("Still offline")).not.toBeInTheDocument();
  lastToastAction().onClick(new MouseEvent("click"));
  await waitFor(() => expect(retry).toHaveBeenNthCalledWith(2, botId, "m4"));
});

test("keeps a refresh failure in one persistent toast with a Retry action", async () => {
  const { send, submit } = await openChat();
  const messages = vi.mocked(testDevice.api.getBotMessages);
  const page = await messages.mock.results[0].value;
  messages.mockRejectedValue(new Error("Refresh failed"));
  send.mockRejectedValue(new Error("Offline"));
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(toast.error).toHaveBeenCalledWith(
      "Refresh failed",
      expect.objectContaining({ id: "bot-refresh-error", duration: Number.POSITIVE_INFINITY }),
    ),
  );
  expect(screen.queryByText("Refresh failed")).not.toBeInTheDocument();
  const refreshToast = toast.error.mock.calls.find(([message]) => message === "Refresh failed")!;
  messages.mockResolvedValue(page);
  const event = new MouseEvent("click", { cancelable: true });
  (refreshToast[1] as { action: ToastAction }).action.onClick(event);
  expect(event.defaultPrevented).toBe(true);
  await waitFor(() => expect(toast.dismiss).toHaveBeenCalledWith("bot-refresh-error"));
});

test("reloads the Bot from the composer and keeps the command available after a busy response", async () => {
  const reload = vi
    .spyOn(testDevice.api, "reloadBot")
    .mockRejectedValueOnce(new ApiError("busy", "state_conflict", 409))
    .mockResolvedValueOnce();
  const { botId, send, scope, draft } = await openChat();
  const editor = screen.getByRole("textbox");
  await userEvent.type(editor, "/reload");
  await userEvent.keyboard("{Enter}");
  await waitFor(() => expect(toast.error).toHaveBeenCalledOnce());
  expect(editor).toHaveTextContent("/reload");
  await userEvent.keyboard("{Enter}");
  await waitFor(() => expect(toast.success).toHaveBeenCalledOnce());
  expect(reload).toHaveBeenNthCalledWith(2, botId);
  expect(editor.textContent).toBe("");
  expect(send).not.toHaveBeenCalled();
  expect(await loadUploadRecords(scope, draft)).toHaveLength(1);
});

test("reloading from Chat actions preserves the pending Bot message", async () => {
  const reload = vi.spyOn(testDevice.api, "reloadBot").mockResolvedValue();
  const { botId, send } = await openChat();
  const editor = screen.getByRole("textbox");
  await userEvent.type(editor, "Keep this draft");
  await userEvent.click(screen.getByRole("button", { name: "Chat actions" }));
  await userEvent.click(await screen.findByText("/reload"));
  await waitFor(() => expect(reload).toHaveBeenCalledExactlyOnceWith(botId));
  expect(editor).toHaveTextContent("Keep this draft");
  expect(send).not.toHaveBeenCalled();
});
