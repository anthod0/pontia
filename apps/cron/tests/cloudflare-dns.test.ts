import { expect, test } from "bun:test";
import { CloudflareDnsProvider } from "../src/cloudflare-dns";

const hostname = "brave-silver-atlas.edge.pontia.dev";

function response(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

test("DNS lookup accepts only the exact canonical A record", async () => {
  const requests: Array<{ url: string; init: RequestInit }> = [];
  const provider = new CloudflareDnsProvider("secret-token", "zone/id", async (input, init) => {
    requests.push({ url: input.toString(), init: init ?? {} });
    return response({ success: true, result: [{ id: "record/id", name: hostname, type: "A" }] });
  });

  await expect(provider.findA(hostname)).resolves.toEqual({ id: "record/id", hostname });
  expect(requests[0]?.url).toBe(
    `https://api.cloudflare.com/client/v4/zones/zone%2Fid/dns_records?type=A&name=${hostname}`,
  );
  expect(requests[0]?.init.headers).toEqual({
    Authorization: "Bearer secret-token",
    "Content-Type": "application/json",
  });
});

test("DNS lookup treats absence as success and rejects ambiguous responses", async () => {
  const absent = new CloudflareDnsProvider("token", "zone", async () =>
    response({ success: true, result: [] }),
  );
  await expect(absent.findA(hostname)).resolves.toBeNull();

  for (const body of [
    { success: false, result: [] },
    { success: true, result: [{ id: "one", name: hostname, type: "A" }, { id: "two" }] },
    { success: true, result: [{ id: "one", name: `other.${hostname}`, type: "A" }] },
    { success: true, result: [{ id: "one", name: hostname, type: "AAAA" }] },
  ]) {
    const provider = new CloudflareDnsProvider("token", "zone", async () => response(body));
    await expect(provider.findA(hostname)).rejects.toThrow();
  }
});

test("DNS deletion targets the previously looked-up record ID and is idempotent", async () => {
  const urls: string[] = [];
  const provider = new CloudflareDnsProvider("token", "zone", async (input) => {
    urls.push(input.toString());
    return response({ success: true });
  });
  await provider.deleteA({ id: "record/id", hostname });
  expect(urls).toEqual(["https://api.cloudflare.com/client/v4/zones/zone/dns_records/record%2Fid"]);

  let requestCount = 0;
  const alreadyDeleted = new CloudflareDnsProvider("token", "zone", async () => {
    requestCount += 1;
    return requestCount === 1
      ? response({ success: false }, 404)
      : response({ success: true, result: [] });
  });
  await expect(alreadyDeleted.deleteA({ id: "gone", hostname })).resolves.toBeUndefined();

  const ambiguous = new CloudflareDnsProvider("token", "zone", async () =>
    response({ success: false }, 404),
  );
  await expect(ambiguous.deleteA({ id: "gone", hostname })).rejects.toThrow();
});
