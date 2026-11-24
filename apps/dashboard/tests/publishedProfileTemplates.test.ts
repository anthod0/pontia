import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import {
  parseTemplatesDocument,
  publishedTemplatesUrl,
} from "../src/pages/agentProfiles/publishedTemplates";
import { builtInProfileTemplates } from "../src/pages/agentProfiles/templates";

const storageKey = "pontia.profileTemplates";
const day = 24 * 60 * 60 * 1000;
// The file uploaded to files.pontia.dev; the built-in templates are its fallback copy.
const publishedFile = resolve(
  import.meta.dirname,
  "../../cloud/files/dashboard/agent-profile-templates/v1.json",
);

const remoteTemplate = {
  id: "release-writer",
  icon: "pencil-simple-line",
  client_type: "pi",
  model: "provider/model",
  use_yolo: false,
  text: {
    en: { name: "Release writer", description: "Writes notes.", system_prompt: "Write." },
    "zh-CN": { name: "发布说明", description: "撰写说明。", system_prompt: "撰写。" },
  },
};
const remoteDocument = { schema_version: 1, templates: [remoteTemplate] };

function respond(body: unknown, status = 200) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(body), { status })),
  );
}

/** A fresh store, as on page load, since the store keeps its value between subscriptions. */
async function loadStore() {
  vi.resetModules();
  return (await import("../src/pages/agentProfiles/publishedTemplates")).profileTemplates;
}

/** The template IDs shown once the background request, if any, has been handled. */
async function settledIds() {
  const store = await loadStore();
  let ids: string[] = [];
  const unsubscribe = store.subscribe((templates) => {
    ids = templates.map((template) => template.id);
  });
  await Promise.allSettled(vi.mocked(fetch).mock.results.map((result) => result.value));
  await new Promise((resolve) => setTimeout(resolve, 10));
  unsubscribe();
  return ids;
}

const builtInIds = builtInProfileTemplates.map((template) => template.id);

beforeEach(() => {
  localStorage.clear();
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

test("the published file matches the built-in templates", () => {
  const document = JSON.parse(readFileSync(publishedFile, "utf8"));
  expect(document.schema_version).toBe(1);
  expect(parseTemplatesDocument(document)).toEqual(builtInProfileTemplates);
});

test("shows the published templates and saves them for a day", async () => {
  respond(remoteDocument);
  expect(get(await loadStore()).map((template) => template.id)).toEqual(builtInIds);
  expect(await settledIds()).toEqual(["release-writer"]);
  expect(fetch).toHaveBeenCalledWith(publishedTemplatesUrl, expect.anything());
  expect(JSON.parse(localStorage.getItem(storageKey)!)).toMatchObject({
    document: remoteDocument,
  });
});

test("uses saved templates without a request while they are less than a day old", async () => {
  localStorage.setItem(
    storageKey,
    JSON.stringify({ fetched_at: Date.now() - day + 60_000, document: remoteDocument }),
  );
  respond({ schema_version: 1, templates: [] });
  expect(await settledIds()).toEqual(["release-writer"]);
  expect(fetch).not.toHaveBeenCalled();
});

test("refreshes saved templates older than a day and keeps them when the refresh fails", async () => {
  localStorage.setItem(
    storageKey,
    JSON.stringify({ fetched_at: Date.now() - day - 1, document: remoteDocument }),
  );
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      throw new TypeError("Failed to fetch");
    }),
  );
  expect(await settledIds()).toEqual(["release-writer"]);
  expect(fetch).toHaveBeenCalledOnce();

  respond({ ...remoteDocument, templates: [{ ...remoteTemplate, id: "release-notes" }] });
  expect(await settledIds()).toEqual(["release-notes"]);
});

test.each([
  ["the request fails", () => Promise.reject(new TypeError("Failed to fetch"))],
  ["the request times out", () => Promise.reject(new DOMException("timed out", "TimeoutError"))],
  ["the server answers with an error", () => Promise.resolve(new Response("", { status: 404 }))],
  ["the body is not JSON", () => Promise.resolve(new Response("<html>"))],
  [
    "the schema version is unknown",
    () => Promise.resolve(Response.json({ ...remoteDocument, schema_version: 2 })),
  ],
  [
    "a template is malformed",
    () =>
      Promise.resolve(
        Response.json({ schema_version: 1, templates: [{ ...remoteTemplate, use_yolo: "yes" }] }),
      ),
  ],
])("keeps the built-in templates when %s", async (_case, answer) => {
  vi.stubGlobal("fetch", vi.fn(answer));
  expect(await settledIds()).toEqual(builtInIds);
  expect(localStorage.getItem(storageKey)).toBeNull();
});

test("ignores saved templates that no longer match the schema", async () => {
  localStorage.setItem(
    storageKey,
    JSON.stringify({ fetched_at: Date.now(), document: { schema_version: 2, templates: [] } }),
  );
  respond({}, 500);
  expect(await settledIds()).toEqual(builtInIds);
  expect(fetch).toHaveBeenCalledOnce();
});

test("rejects documents with missing text or duplicate IDs and drops unknown clients", () => {
  const withoutChinese = { ...remoteTemplate, text: { en: remoteTemplate.text.en } };
  expect(parseTemplatesDocument({ schema_version: 1, templates: [withoutChinese] })).toBe(
    undefined,
  );
  expect(
    parseTemplatesDocument({ schema_version: 1, templates: [remoteTemplate, remoteTemplate] }),
  ).toBe(undefined);
  expect(
    parseTemplatesDocument({
      schema_version: 1,
      templates: [remoteTemplate, { ...remoteTemplate, id: "other", client_type: "future" }],
    }),
  ).toEqual([remoteTemplate]);
  expect(parseTemplatesDocument({ schema_version: 1, templates: [] })).toBe(undefined);
});
