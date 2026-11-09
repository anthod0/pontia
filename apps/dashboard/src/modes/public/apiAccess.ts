import type { ApiAccess } from "../../device/apiAccess";
import type { RemoteAccess } from "./remoteAccessLoader.svelte";

type RemoteConnection = ReturnType<RemoteAccess["createRemoteConnection"]>;

export function createApiAccess(connection: RemoteConnection): ApiAccess {
  return {
    apiCredentials: "include",
    apiStreamUnavailableMessage: "Remote device access is not ready.",
    apiUrl: (path) => path,
    apiFetch: (path, init) => connection.e2eFetch(path, init),
    apiSignal: (signal) =>
      signal ? AbortSignal.any([connection.signal, signal]) : connection.signal,
    applyApiAuthentication(headers) {
      headers.delete("Authorization");
      return true;
    },
    handleApiAuthenticationFailure() {},
    uploadStorageScope: connection.uploadStorageScope,
    dispose: connection.dispose,
  };
}
