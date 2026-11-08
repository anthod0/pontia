import { get } from "svelte/store";
import { DeviceRuntime } from "../src/device/runtime";
import { token } from "../src/stores/auth";

// Existing single-device tests configure authentication after importing their modules.
export const testDevice = new DeviceRuntime("local", {
  apiCredentials: "same-origin",
  apiStreamUnavailableMessage: "Set an API token in Settings to open the dashboard event stream.",
  apiUrl: (path) => path,
  apiFetch: (path, init) => fetch(path, init),
  apiSignal: (signal) => signal ?? new AbortController().signal,
  applyApiAuthentication(headers) {
    const bearer = get(token).trim();
    if (!bearer) return false;
    headers.set("Authorization", `Bearer ${bearer}`);
    return true;
  },
  handleApiAuthenticationFailure: () => token.set(""),
  uploadStorageScope: async () => JSON.stringify([location.origin, get(token)]),
  dispose() {},
});
