import { Blob, File } from "node:buffer";
import { get } from "svelte/store";
import { afterEach, expect, test, vi } from "vitest";
import { testDevice } from "./deviceFixture";
import { loadUploadRecords } from "../src/lib/uploads/records";
import { maskSecret, secretSizeValid } from "../src/lib/uploads/secretMask";
import { uploadMessage, parseUploadMessage } from "../src/lib/uploads/message";
import { titleFromInitialPrompt } from "../src/lib/session-chat/sessionChat";
import { entryText } from "../src/lib/botQuote";
import { searchSnippet } from "../src/lib/botSearch";
import type { BotMessage } from "../src/api/types";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
test.each([
  ["x", "*******"],
  ["12345678", "*******"],
  ["123456789", "12*******89"],
  ["123456789012345", "12*******45"],
  ["1234567890123456", "1234*******456"],
  ["🔑".repeat(8), "*******"],
  ["🗝🔑12345🔒🔓", "🗝🔑*******🔒🔓"],
  ["ab1234567890\x7f", "*******"],
  ["-----BEGIN PRIVATE KEY-----\r\nabc\r\n", "PEM(PRIVATE KEY, *******, 2 lines)"],
  ["abc\rdef\r\r", "******* (3 lines)"],
  ["{\n}\n", "******* (2 lines)"],
])("masks %j", (value, expected) => expect(maskSecret(value)).toBe(expected));

test("limits UTF-8 bytes without trimming the secret", () => {
  expect(secretSizeValid("")).toBe(false);
  expect(secretSizeValid(" ")).toBe(true);
  const limit = "🔑".repeat(16384);
  expect(secretSizeValid(limit)).toBe(true);
  expect(secretSizeValid(limit + "a")).toBe(false);
});

test("round trips mixed attachments and treats malformed secret blocks as literal text", () => {
  const files = [{ name: "oak.txt", path: "/files/oak.txt" }];
  const secrets = [
    { path: "/a&b", label: '"Dev" & ops', masked: "<&*******x>" },
    { path: "/b", masked: "*******" },
  ];
  const content = uploadMessage("body", files, secrets);
  expect(parseUploadMessage(content)).toEqual({ text: "body", files, secrets });
  for (const child of [
    '<user_secret path="rel">a</user_secret>',
    '<user_secret path="/a" label="">a</user_secret>',
    '<user_secret path="/a" label="x&#9;y">a</user_secret>',
    '<user_secret path="/a" extra="x">a</user_secret>',
    '<user_secret path="/a"><b>x</b></user_secret>',
    '<user_secret path="/a"></user_secret>',
  ]) {
    const invalid = content.replace(/<user_secret [^\n]+/, child);
    expect(parseUploadMessage(invalid)).toEqual({ text: invalid, files: [], secrets: [] });
  }
});

test("uses visible secret metadata for session titles, Bot quotes and search snippets", () => {
  const secret = { path: "/private/path", label: "Dev", masked: "ab*******cd" };
  const content = uploadMessage("", [], [secret]);
  expect(titleFromInitialPrompt(content)).toBe("Dev");
  expect(titleFromInitialPrompt(uploadMessage("", [], [{ ...secret, label: undefined }]))).toBe(
    "Secret",
  );
  const msg: BotMessage = {
    id: "id",
    kind: "user_input",
    created_at: "",
    status: "normal",
    content:
      '<user_quote message_id="bmsg_0193a000-0000-7000-8000-000000000002" speaker="Bot">Ready</user_quote>\n' +
      content,
  };
  expect(entryText(msg)).toBe("Dev (ab*******cd)");
  expect(searchSnippet(msg, "Dev")).toEqual([{ text: "Dev", match: true }]);
  expect(searchSnippet(msg, "*******")).toContainEqual({ text: "*******", match: true });
  const markupMatch = searchSnippet(msg, "private")
    .map((p) => p.text)
    .join("");
  expect(markupMatch).not.toContain(secret.path);
  expect(markupMatch).not.toContain("user_secrets");
});

test("uploads original secret bytes with a neutral name, leaves no recovery record and releases the file", async () => {
  vi.stubGlobal("File", File);
  const scope = "secret-test";
  const id = crypto.randomUUID();
  const source = " abc\r\ndef\n ";
  testDevice.access.uploadStorageScope = async () => scope;
  let bytes: ArrayBuffer | undefined;
  let name = "";
  vi.stubGlobal("fetch", async (url: string, init: RequestInit) => {
    const parsed = new URL(url, "http://localhost");
    const key = new Headers(init.headers).get("Idempotency-Key");
    if (key) {
      name = parsed.searchParams.get("name")!;
      expect(name).toBe(`sec_${key.slice(0, 8)}.secret`);
      expect(await loadUploadRecords(scope, id)).toEqual([]);
    }
    if (init.method === "PATCH") bytes = init.body as ArrayBuffer;
    const completed = parsed.pathname.endsWith("/complete");
    const upload = {
      upload_id: id,
      name,
      size: source.length,
      offset: bytes?.byteLength ?? 0,
      completed,
      path: completed ? "/files/" + name : null,
    };
    return new Response(JSON.stringify({ data: { upload }, error: null, meta: {} }));
  });
  const draft = testDevice.uploadDrafts.createUploadDraft(id);
  await draft.initialized;
  vi.spyOn(Blob.prototype, "arrayBuffer").mockRejectedValueOnce(new Error("read failed"));
  draft.addSecret(source, " Dev ");
  await vi.waitFor(() => expect(get(draft).items[0].status).toBe("failed"));
  await draft.retry(get(draft).items[0]);
  await vi.waitFor(() => expect(get(draft).items[0].status).toBe("complete"));
  expect(new TextDecoder().decode(bytes)).toBe(source);
  expect(get(draft).items[0].file).toBeUndefined();
  expect(get(draft).items[0].secret).toEqual({ label: "Dev", masked: "******* (3 lines)" });
  expect(await loadUploadRecords(scope, id)).toEqual([]);
  await draft.clear();
  draft.destroy();
});
