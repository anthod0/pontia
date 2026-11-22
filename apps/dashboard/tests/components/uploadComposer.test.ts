import { testDevice } from "../deviceFixture";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
const toast = vi.hoisted(() => ({ error: vi.fn(), warning: vi.fn(), dismiss: vi.fn() }));
vi.mock("svelte-sonner", () => ({ toast }));
import MessageComposer from "../../src/components/chat/MessageComposer.svelte";
import { loadUploadRecords, saveUploadRecord } from "../../src/lib/uploads/records";
testDevice.access.uploadStorageScope = async () => "composer-test";
afterEach(() => vi.unstubAllGlobals());
async function seed(completed: boolean) {
  const draft = crypto.randomUUID();
  const record = {
    scope: "composer-test",
    draft,
    key: crypto.randomUUID(),
    uploadId: crypto.randomUUID(),
    name: "report.txt",
    size: 3,
    sha256: "a".repeat(64),
  };
  await saveUploadRecord(record);
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async () =>
        new Response(
          JSON.stringify({
            data: {
              upload: {
                upload_id: record.uploadId,
                name: record.name,
                size: 3,
                sha256: record.sha256,
                offset: completed ? 3 : 0,
                completed,
                path: completed ? "/device/report.txt" : null,
                created_at: "2026-11-03T00:00:00Z",
              },
            },
            meta: {},
            error: null,
          }),
        ),
    ),
  );
  return draft;
}
test("sends attachment-only messages and clears recovery only after successful submission", async () => {
  const draftId = await seed(true);
  const onSubmit = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  render(MessageComposer, { value: "", draftId, onSubmit });
  const submit = screen.getByRole("button", { name: "Send" });
  await waitFor(() => expect(submit).toBeEnabled());
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(onSubmit).toHaveBeenCalledWith(
      '<user_uploads note="Temporary files; copy any you need into the workspace.">\n<user_upload path="/device/report.txt">report.txt</user_upload>\n</user_uploads>',
      "steer",
    ),
  );
  expect(await loadUploadRecords("composer-test", draftId)).toHaveLength(1);
  await waitFor(() => expect(submit).toBeEnabled());
  await fireEvent.click(submit);
  await waitFor(async () => expect(await loadUploadRecords("composer-test", draftId)).toEqual([]));
  expect(submit).toBeDisabled();
});
test("blocks sending unfinished attachments until they are removed", async () => {
  const draftId = await seed(false);
  const onSubmit = vi.fn();
  render(MessageComposer, { value: "Read the report", draftId, onSubmit });
  await screen.findByRole("button", { name: "Remove attachment report.txt" });
  const submit = screen.getByRole("button", { name: "Send" });
  expect(submit).toBeDisabled();
  await fireEvent.click(screen.getByRole("button", { name: "Remove attachment report.txt" }));
  await waitFor(() => expect(submit).toBeEnabled());
  await fireEvent.click(submit);
  expect(onSubmit).toHaveBeenCalledWith("Read the report", "steer");
});

test("reports upload failures in a toast instead of beside the file", async () => {
  const draftId = await seed(true);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      throw new Error("device unreachable");
    }),
  );
  render(MessageComposer, { value: "", draftId, onSubmit: vi.fn() });
  await screen.findByRole("img", { name: "Upload failed: report.txt" });
  expect(screen.queryByText("device unreachable")).not.toBeInTheDocument();
  await waitFor(() => expect(toast.error).toHaveBeenCalledTimes(1));
  expect(toast.error).toHaveBeenCalledWith(
    "Upload failed: report.txt",
    expect.objectContaining({ description: expect.stringContaining("device unreachable") }),
  );
});

function clipboard(files: File[], types: string[]) {
  return { files, types: [...types, ...(files.length ? ["Files"] : [])], getData: () => "" };
}

test("uploads files pasted into the input", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise(() => {})),
  );
  const draftId = crypto.randomUUID();
  render(MessageComposer, { value: "", draftId, onSubmit: vi.fn() });
  const send = screen.getByRole("button", { name: "Send" });
  await waitFor(() => expect(screen.getByRole("button", { name: "Upload files" })).toBeEnabled());
  const file = new File(["png"], "image.png", { type: "image/png" });
  const pasted = await fireEvent.paste(screen.getByRole("textbox"), {
    clipboardData: clipboard([file], []),
  });
  expect(pasted).toBe(false);
  await screen.findByRole("button", { name: "Remove attachment image.png" });
  expect(send).toBeDisabled();
});

test("keeps rich text pastes that carry a rendered image as text", async () => {
  const draftId = crypto.randomUUID();
  render(MessageComposer, { value: "", draftId, onSubmit: vi.fn() });
  await waitFor(() => expect(screen.getByRole("button", { name: "Upload files" })).toBeEnabled());
  const file = new File(["png"], "image.png", { type: "image/png" });
  const pasted = await fireEvent.paste(screen.getByRole("textbox"), {
    clipboardData: clipboard([file], ["text/plain", "text/rtf"]),
  });
  expect(pasted).toBe(true);
  expect(
    screen.queryByRole("button", { name: "Remove attachment image.png" }),
  ).not.toBeInTheDocument();
});

test("reports a failed attachment cleanup in a persistent toast whose Retry action clears it", async () => {
  const draftId = await seed(true);
  const onSubmit = vi.fn().mockResolvedValue(true);
  render(MessageComposer, { value: "", draftId, onSubmit });
  const submit = screen.getByRole("button", { name: "Send" });
  await waitFor(() => expect(submit).toBeEnabled());
  const records = await import("../../src/lib/uploads/records");
  const remove = vi
    .spyOn(records, "removeUploadRecord")
    .mockRejectedValueOnce(new Error("storage locked"));
  toast.error.mockClear();
  await fireEvent.click(submit);
  await waitFor(() =>
    expect(toast.error).toHaveBeenCalledExactlyOnceWith(
      "storage locked",
      expect.objectContaining({
        id: `composer-submit-error:${draftId}`,
        duration: Number.POSITIVE_INFINITY,
        action: expect.objectContaining({ label: "Retry" }),
      }),
    ),
  );
  expect(screen.queryByText("storage locked")).not.toBeInTheDocument();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  await screen.findByRole("button", { name: "Dismiss sent attachments" });
  remove.mockRestore();
  (toast.error.mock.lastCall![1] as { action: { onClick: () => void } }).action.onClick();
  await waitFor(async () => expect(await loadUploadRecords("composer-test", draftId)).toEqual([]));
  expect(onSubmit).toHaveBeenCalledOnce();
  expect(toast.dismiss).toHaveBeenCalledWith(`composer-submit-error:${draftId}`);
});
