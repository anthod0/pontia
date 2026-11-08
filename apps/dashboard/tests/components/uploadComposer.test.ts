import { testDevice } from "../deviceFixture";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
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
      '<user_upload path="/device/report.txt">report.txt</user_upload>',
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
  expect(onSubmit).toHaveBeenCalledWith("Read the report");
});
