import { expect, test } from "vitest";
import { createSampleData } from "../mock/data";
import { createMockDevice, handleDeviceRequest, type MockDevice } from "../mock/server";
import type { SessionCreationOption } from "../src/api/types";

async function request(device: MockDevice, method: string, path: string, body?: unknown) {
  const response = await handleDeviceRequest(
    device,
    new Request(`http://localhost/api/v1${path}`, {
      method,
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
    path,
  );
  expect(response.status).toBe(200);
  return (await response.json()).data;
}

async function optionIds(device: MockDevice) {
  const { options } = await request(device, "GET", "/session-creation-options");
  return (options as SessionCreationOption[]).map((option) => option.id);
}

test.each([
  ["pi", "default", "pi"],
  ["codex", "default", "codex"],
  ["claude-agent", "default", "claude-agent"],
  ["acp", "antigravity", '["acp","antigravity"]'],
  ["acp", "grok", '["acp","grok"]'],
])("mock creation options follow enabled settings for %s", async (type, key, id) => {
  const device = createMockDevice(createSampleData());
  const otherDevice = createMockDevice(createSampleData());
  const path = `/agent-clients/${type}/${key}`;
  const allIds = ["pi", "codex", "claude-agent", '["acp","antigravity"]', '["acp","grok"]'];

  await request(device, "PATCH", path, { enabled: false });
  expect(await optionIds(device)).toEqual(allIds.filter((value) => value !== id));
  expect(await optionIds(otherDevice)).toEqual(allIds);

  await request(device, "PATCH", path, { enabled: true });
  expect(await optionIds(device)).toEqual(allIds);

  await request(device, "DELETE", path);
  expect(await optionIds(device)).toEqual(allIds.filter((value) => value !== id));
  const { agent_clients } = await request(device, "GET", "/agent-clients");
  expect(agent_clients).toContainEqual(
    expect.objectContaining({
      client_type: type,
      client_key: key,
      enabled: false,
      version: null,
      version_checked_at: null,
      has_record: false,
    }),
  );
});

test("mock creation options carry the client version", async () => {
  const device = createMockDevice(createSampleData());
  const { options } = await request(device, "GET", "/session-creation-options");
  expect(
    Object.fromEntries(
      (options as SessionCreationOption[]).map((option) => [option.id, option.version]),
    ),
  ).toEqual({
    pi: "0.70.2",
    codex: "0.171.0",
    "claude-agent": "2.1.318",
    '["acp","antigravity"]': null,
    '["acp","grok"]': "1.0.92",
  });
});

test("mock creation options are empty when every client is disabled", async () => {
  const device = createMockDevice(createSampleData());
  for (const path of [
    "pi/default",
    "codex/default",
    "claude-agent/default",
    "acp/antigravity",
    "acp/grok",
  ]) {
    await request(device, "PATCH", `/agent-clients/${path}`, { enabled: false });
  }
  expect(await optionIds(device)).toEqual([]);
});
