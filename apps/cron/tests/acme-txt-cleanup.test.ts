import { afterEach, expect, mock, spyOn, test } from "bun:test";
import { CloudflareDnsProvider } from "../src/cloudflare-dns";

afterEach(() => mock.restore());

test("TXT cleanup uses provider creation time and record ID, never deletes a fresh challenge or A record", async () => {
  const now = new Date("2026-10-10T00:00:00Z");
  const records = [
    {
      id: "old",
      type: "TXT",
      name: "_acme-challenge.brave-atlas.edge.pontia.dev",
      created_on: "2026-10-08T23:59:59Z",
    },
    {
      id: "new",
      type: "TXT",
      name: "_acme-challenge.brave-atlas.edge.pontia.dev",
      created_on: "2026-10-09T23:00:00Z",
    },
    {
      id: "boundary",
      type: "TXT",
      name: "_acme-challenge.brave-atlas.edge.pontia.dev",
      created_on: "2026-10-09T00:00:00Z",
    },
    {
      id: "other",
      type: "TXT",
      name: "_acme-challenge.other.example",
      created_on: "2026-01-01T00:00:00Z",
    },
    {
      id: "nested",
      type: "TXT",
      name: "_acme-challenge.one.two.edge.pontia.dev",
      created_on: "2026-01-01T00:00:00Z",
    },
    {
      id: "invalid",
      type: "TXT",
      name: "_acme-challenge.brave-atlas.edge.pontia.dev",
      created_on: "invalid",
    },
    { id: "A", type: "A", name: "brave-atlas.edge.pontia.dev", created_on: "2026-01-01T00:00:00Z" },
  ];
  const deleted: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation((async (input, init) => {
    if (init?.method === "DELETE") {
      deleted.push(String(input).split("/").at(-1)!);
      return Response.json({ success: true });
    }
    return Response.json({ success: true, result: records });
  }) as typeof fetch);
  expect(await new CloudflareDnsProvider("secret", "zone").cleanupExpiredTxt(now)).toBe(1);
  expect(deleted).toEqual(["old"]);
});

test("TXT cleanup enumerates all pages before deleting records", async () => {
  const events: string[] = [];
  spyOn(globalThis, "fetch").mockImplementation((async (input, init) => {
    const url = new URL(String(input));
    if (init?.method === "DELETE") {
      events.push("delete");
      return Response.json({ success: true });
    }
    const page = url.searchParams.get("page");
    events.push(`page:${page}`);
    return Response.json({
      success: true,
      result:
        page === "1"
          ? Array.from({ length: 20 }, (_, index) => ({
              id: `old-${index}`,
              type: "TXT",
              name: "_acme-challenge.brave-atlas.edge.pontia.dev",
              created_on: "2026-01-01T00:00:00Z",
            }))
          : [],
    });
  }) as typeof fetch);
  expect(
    await new CloudflareDnsProvider("secret", "zone").cleanupExpiredTxt(new Date("2026-10-10")),
  ).toBe(20);
  expect(events.slice(0, 2)).toEqual(["page:1", "page:2"]);
  expect(events.slice(2)).toEqual(Array(20).fill("delete"));
});
