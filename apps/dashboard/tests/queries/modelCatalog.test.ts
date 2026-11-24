import { afterEach, expect, test, vi } from "vitest";
import { modelCatalogQuery } from "../../src/queries/modelCatalog";
import { DeviceRuntime } from "../../src/device/runtime";
import { testDevice } from "../deviceFixture";

const client = { client_type: "acp", client_key: "example", version: "1" };
const entry = {
  version: "1",
  fetched_at: Date.now(),
  default_model: "first",
  models: [
    {
      id: "first",
      name: "First",
      description: "",
      reasoning_efforts: [{ id: "high", name: "High", description: "" }],
      default_reasoning_effort: "high",
    },
  ],
};
afterEach(() => vi.restoreAllMocks());

test("reuses the complete persistent catalog across selectors on the same device", async () => {
  const api = vi.spyOn(testDevice.api, "listAgentClientModels").mockResolvedValue(entry);
  await testDevice.queryClient.fetchQuery(modelCatalogQuery(testDevice, client));
  testDevice.queryClient.clear();
  const query = modelCatalogQuery(testDevice, client, "other-workspace");
  expect(query.initialData()).toEqual(entry);
  expect(await testDevice.queryClient.fetchQuery(query)).toEqual(entry);
  expect(api).toHaveBeenCalledTimes(1);
  const otherDevice = new DeviceRuntime("another-device", testDevice.access);
  try {
    expect(modelCatalogQuery(otherDevice, client).initialData()).toBeUndefined();
    expect(
      modelCatalogQuery(testDevice, { ...client, client_key: "other" }).initialData(),
    ).toBeUndefined();
  } finally {
    otherDevice.dispose();
  }
});

test.each(["expired", "version change"])(
  "displays %s data throughout background refresh",
  async (cause) => {
    const stale = { ...entry, fetched_at: cause === "expired" ? 1 : Date.now() };
    const selected = cause === "expired" ? client : { ...client, version: "2" };
    const latest = {
      ...entry,
      version: selected.version,
      fetched_at: Date.now(),
      default_model: null,
      models: [],
    };
    const pending = Promise.withResolvers<typeof latest>();
    const api = vi
      .spyOn(testDevice.api, "listAgentClientModels")
      .mockResolvedValueOnce(stale)
      .mockReturnValueOnce(pending.promise);
    const query = modelCatalogQuery(testDevice, selected);
    const refresh = testDevice.queryClient.fetchQuery(query);
    await vi.waitFor(() =>
      expect(testDevice.queryClient.getQueryData(query.queryKey)).toEqual(stale),
    );
    expect(query.initialData()).toEqual(stale);
    expect(api.mock.calls.map((call) => call[4])).toEqual([true, false]);
    pending.resolve(latest);
    expect(await refresh).toEqual(latest);
    expect(query.initialData()).toEqual(latest);
  },
);
