import { describe, expect, test, vi } from "vitest";
import { fetchJson, optionalString, parseJsonResponse, responseDataRecord } from "../src/internal-api.js";

describe("pi internal api helpers", () => {
  test("parses JSON responses and falls back to text/null", async () => {
    await expect(parseJsonResponse(new Response(JSON.stringify({ ok: true })))).resolves.toEqual({ ok: true });
    await expect(parseJsonResponse(new Response("plain"))).resolves.toBe("plain");
    await expect(parseJsonResponse(new Response(""))).resolves.toBeNull();
  });

  test("fetchJson sends bearer token and throws on non-ok responses", async () => {
    const fetchImpl = vi.fn(async () => new Response(JSON.stringify({ data: { ok: true } }), { status: 200 }));
    await expect(fetchJson(fetchImpl as any, "http://example.test", "token")).resolves.toEqual({ data: { ok: true } });
    expect(fetchImpl).toHaveBeenCalledWith("http://example.test", { headers: { Authorization: "Bearer token" } });

    await expect(fetchJson((async () => new Response("no", { status: 500, statusText: "Broken" })) as any, "u", "t")).rejects.toThrow("500 Broken");
  });

  test("extracts data records and optional strings", () => {
    expect(responseDataRecord({ data: { value: 1 } })).toEqual({ value: 1 });
    expect(responseDataRecord({ data: [] })).toBeUndefined();
    expect(optionalString(" value ")).toBe(" value ");
    expect(optionalString("   ")).toBeUndefined();
    expect(optionalString(42)).toBeUndefined();
  });
});
