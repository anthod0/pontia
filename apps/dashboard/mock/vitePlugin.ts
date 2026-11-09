import type { IncomingMessage, ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import type { Plugin } from "vite";
import { createEmptyData, createSampleData } from "./data.ts";
import { mockDataKind, mockDeviceHeader, mockLocalDevice } from "./devices.ts";
import { createMockDevice, handleDeviceRequest, type MockDevice } from "./server.ts";

const remoteAccessEntry = fileURLToPath(
  new URL("../src/modes/public/remote-access/index.ts", import.meta.url),
);
const mockRemoteAccess = fileURLToPath(new URL("./remoteAccess.ts", import.meta.url));

// Connect strips the `/api/v1` mount path from `req.url`; only the query is read from it.
async function toRequest(req: IncomingMessage, signal: AbortSignal): Promise<Request> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) chunks.push(chunk as Buffer);
  const headers = new Headers();
  for (const [name, value] of Object.entries(req.headers)) {
    if (typeof value === "string") headers.set(name, value);
  }
  return new Request(`http://${req.headers.host ?? "localhost"}${req.url}`, {
    method: req.method,
    headers,
    body: chunks.length ? Buffer.concat(chunks) : undefined,
    signal,
  });
}

async function sendResponse(response: Response, res: ServerResponse): Promise<void> {
  res.writeHead(response.status, Object.fromEntries(response.headers));
  res.flushHeaders();
  if (!response.body) return void res.end();
  const reader = response.body.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      res.write(value);
    }
  } finally {
    res.end();
  }
}

/**
 * Serves the pontiad `/api/v1` API from in-memory mock devices during `vite dev`.
 * In the Public Dashboard it also swaps remote access for a mocked Pontia cloud.
 */
export function mockApi(options: { publicDashboard: boolean }): Plugin {
  const devices = new Map<string, MockDevice>();

  function device(handle: string): MockDevice {
    let found = devices.get(handle);
    if (!found) {
      found = createMockDevice(
        mockDataKind(handle) === "empty" ? createEmptyData() : createSampleData(),
      );
      devices.set(handle, found);
    }
    return found;
  }

  return {
    name: "pontia-mock-api",
    apply: "serve",
    enforce: "pre",
    config: () => ({ server: { fs: { allow: [fileURLToPath(new URL(".", import.meta.url))] } } }),
    async resolveId(source, importer) {
      if (!options.publicDashboard || !importer) return null;
      const resolved = await this.resolve(source, importer, { skipSelf: true });
      return resolved?.id === remoteAccessEntry ? mockRemoteAccess : null;
    },
    configureServer(server) {
      server.middlewares.use("/api/v1", (req, res, next) => {
        const path = (req.url ?? "/").split("?")[0];
        const handle = req.headers[mockDeviceHeader];
        const target = device(typeof handle === "string" ? handle : mockLocalDevice);
        const lifetime = new AbortController();
        res.on("close", () => lifetime.abort());
        toRequest(req, lifetime.signal)
          .then((request) => handleDeviceRequest(target, request, path))
          .then((response) => sendResponse(response, res))
          .catch(next);
      });
    },
  };
}
