/** Devices returned by the mocked Pontia cloud in the Public Dashboard. */
export const mockCloudDevices = [
  { handle: "studio", name: "Studio workstation", connection: "online", data: "sample" },
  { handle: "laptop", name: "New laptop", connection: "online", data: "empty" },
  { handle: "nas-box", name: "NAS box", connection: "offline", data: "sample" },
  { handle: "edge-lab", name: "Edge lab", connection: "connecting", data: "sample" },
] as const;

/** The device the Local Dashboard talks to. */
export const mockLocalDevice = "local";

/** Request header that selects the mocked device behind `/api/v1`. */
export const mockDeviceHeader = "x-pontia-mock-device";

export function mockDataKind(handle: string): "sample" | "empty" {
  return mockCloudDevices.find((device) => device.handle === handle)?.data ?? "sample";
}
