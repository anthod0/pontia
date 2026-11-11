import { testDevice } from "./deviceFixture";
import { File, Blob } from "node:buffer";
import { get } from "svelte/store";
import { afterEach, expect, test, vi } from "vitest";
import { fileDigest } from "../src/lib/uploads/draft";
const { createUploadDraft } = testDevice.uploadDrafts;
import { loadUploadRecords, saveUploadRecord } from "../src/lib/uploads/records";
import { parseUploadMessage, uploadMessage } from "../src/lib/uploads/message";
testDevice.access.uploadStorageScope = async () => "user:device";
afterEach(() => vi.unstubAllGlobals());
const file = (content = "abc") => new File([content], "log.txt") as unknown as globalThis.File;
const digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
function server(draft: string, lost: "create" | "chunk") {
  let upload: any = null;
  let loseResponse = true;
  const patches: number[] = [];
  const fetch = vi.fn(async (url: string, init: RequestInit) => {
    const parsed = new URL(url, "http://localhost");
    const id = upload?.upload_id ?? new Headers(init.headers).get("Idempotency-Key");
    if (init.method === "POST" && parsed.pathname.endsWith("uploads")) {
      const records = await loadUploadRecords("user:device", draft);
      expect(records.some((record) => record.key === id)).toBe(true);
      upload ??= {
        upload_id: id,
        name: "log.txt",
        size: 3,
        sha256: digest,
        offset: 0,
        completed: false,
        path: null,
        created_at: "2026-11-03T00:00:00Z",
      };
      if (lost === "create" && loseResponse) {
        loseResponse = false;
        throw new TypeError("offline");
      }
    } else if (init.method === "PATCH") {
      const offset = Number(parsed.searchParams.get("offset"));
      patches.push(offset);
      expect(offset).toBe(upload.offset);
      upload.offset += (init.body as ArrayBuffer).byteLength;
      if (lost === "chunk" && loseResponse) {
        loseResponse = false;
        throw new TypeError("offline");
      }
    } else if (parsed.pathname.endsWith("complete")) {
      upload.completed = true;
      upload.path = "/device/uploads/log.txt";
    }
    return new Response(JSON.stringify({ data: { upload }, error: null, meta: {} }));
  });
  vi.stubGlobal("fetch", fetch);
  return { fetch, patches };
}

test("hashes file bytes using SHA-256", async () => {
  expect(await fileDigest(file())).toBe(digest);
  expect(await fileDigest(new Blob([]) as unknown as globalThis.Blob)).toBe(
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  );
});

test("recovers lost creation with the persisted key and verifies the reselected file", async () => {
  const id = crypto.randomUUID();
  const remote = server(id, "create");
  const first = createUploadDraft(id);
  await first.initialized;
  await first.add([file()]);
  await vi.waitFor(() => expect(get(first).items[0].status).toBe("failed"));
  const key = get(first).items[0].id;
  first.destroy();
  const resumed = createUploadDraft(id);
  await resumed.initialized;
  expect(get(resumed).items[0].id).toBe(key);
  expect(get(resumed).items[0].status).toBe("select");
  await resumed.retry(get(resumed).items[0], file("xyz"));
  expect(remote.patches).toEqual([]);
  await resumed.retry(get(resumed).items[0], file());
  expect(get(resumed).items[0].status).toBe("complete");
  await resumed.clear();
  resumed.destroy();
  expect(await loadUploadRecords("user:device", id)).toEqual([]);
});

test("queries progress after losing a chunk response and restores completed references without a file", async () => {
  const id = crypto.randomUUID();
  const remote = server(id, "chunk");
  const first = createUploadDraft(id);
  await first.initialized;
  await first.add([file()]);
  await vi.waitFor(() => expect(get(first).items[0].status).toBe("failed"));
  await first.retry(get(first).items[0]);
  expect(remote.patches).toEqual([0]);
  expect(get(first).items[0].status).toBe("complete");
  first.destroy();
  const restored = createUploadDraft(id);
  await restored.initialized;
  expect(get(restored).items[0].upload?.path).toBe("/device/uploads/log.txt");
  expect(get(restored).items[0].file).toBeUndefined();
  restored.destroy();
});

test("isolates recovery records by user, device and draft", async () => {
  const draft = crypto.randomUUID();
  const base = {
    scope: "alice:device-a",
    draft,
    key: crypto.randomUUID(),
    uploadId: null,
    name: "x",
    size: 0,
    sha256: digest,
  };
  await saveUploadRecord(base);
  expect(await loadUploadRecords("bob:device-a", draft)).toEqual([]);
  expect(await loadUploadRecords("alice:device-b", draft)).toEqual([]);
  expect(await loadUploadRecords(base.scope, "another-chat")).toEqual([]);
  expect(await loadUploadRecords(base.scope, draft)).toEqual([base]);
});

test("round trips escaped attachments and retains malformed tags as text", () => {
  const files = [{ path: '/home/a&b/"report"', name: 'report & "notes" <x>\n.txt' }];
  expect(parseUploadMessage(uploadMessage("Read these", files))).toEqual({
    text: "Read these",
    files,
  });
  expect(uploadMessage("Read these", files)).toMatch(
    /^<user_upload [^\n]*<\/user_upload>\nRead these$/,
  );
  const trailing = 'Read these\n<user_upload path="/device/file">report.txt</user_upload>';
  expect(parseUploadMessage(trailing)).toEqual({ text: trailing, files: [] });
  const invalid =
    '<user_upload path="relative">x</user_upload>\n<user_upload path="/x">&unknown;</user_upload>\n<user_upload path="/x"><script>bad</script></user_upload>';
  expect(parseUploadMessage(invalid)).toEqual({ text: invalid, files: [] });
});

test("removed references stay removed when IndexedDB deletion fails", async () => {
  const { removeUploadRecord } = await import("../src/lib/uploads/records");
  const record = {
    scope: "cleanup",
    draft: crypto.randomUUID(),
    key: crypto.randomUUID(),
    uploadId: null,
    name: "x",
    size: 0,
    sha256: digest,
  };
  await saveUploadRecord(record);
  const deletion = vi.spyOn(IDBObjectStore.prototype, "delete").mockImplementation(() => {
    throw new Error("storage unavailable");
  });
  try {
    await removeUploadRecord(record);
  } finally {
    deletion.mockRestore();
  }
  expect(await loadUploadRecords(record.scope, record.draft)).toEqual([]);
});

test("uploads on Local HTTP origins without secure-context crypto APIs", async () => {
  const { createApiAccess } = await import("../src/modes/local/apiAccess");
  const { uploadStorageScope } = createApiAccess();
  const draftId = crypto.randomUUID();
  const random = crypto.getRandomValues.bind(crypto);
  vi.stubGlobal("crypto", { getRandomValues: random });
  await expect(uploadStorageScope()).resolves.toEqual(expect.any(String));
  const remote = server(draftId, "chunk");
  const draft = createUploadDraft(draftId);
  await draft.initialized;
  await draft.add([file()]);
  await vi.waitFor(() => expect(get(draft).items[0].status).toBe("failed"));
  expect(get(draft).items[0].id).toMatch(
    /^[\da-f]{8}-[\da-f]{4}-4[\da-f]{3}-[89ab][\da-f]{3}-[\da-f]{12}$/,
  );
  await draft.retry(get(draft).items[0]);
  expect(remote.patches).toEqual([0]);
  expect(get(draft).items[0].status).toBe("complete");
  draft.destroy();
});

test("recognizes valid quoted XML attributes while keeping extra attributes and nested markup literal", () => {
  expect(
    parseUploadMessage("<user_upload path = '/device/file'>report.txt</user_upload>").files,
  ).toEqual([{ path: "/device/file", name: "report.txt" }]);
  const text = '<user_upload path="/device/file" onclick="bad">report.txt</user_upload>';
  expect(parseUploadMessage(text)).toEqual({ text, files: [] });
});
