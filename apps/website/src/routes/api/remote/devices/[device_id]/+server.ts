import { json } from "@sveltejs/kit";
import { cliPrincipal, remoteDatabase } from "$lib/server/remote-access/http";
import { findRegisteredDevice, registerDevice } from "$lib/server/remote-access/registration";
import type { RequestHandler } from "./$types";

function deviceResponseBody(device: {
  id: string;
  name: string | null;
  edgeId: string;
  edgeName: string;
}) {
  return {
    id: device.id,
    name: device.name,
    edge_id: device.edgeId,
    edge_name: device.edgeName,
  };
}

export const GET: RequestHandler = async (event) => {
  const principal = await cliPrincipal(event);
  if (!principal) return json({ error: "invalid_credentials" }, { status: 401 });
  const result = await findRegisteredDevice(
    remoteDatabase(event),
    principal.userId,
    event.params.device_id,
  );
  if (result.status === "not_found") return json({ error: "device_not_found" }, { status: 404 });
  if (result.status === "conflict") return json({ error: "device_conflict" }, { status: 409 });
  return json(deviceResponseBody(result.device));
};

export const PUT: RequestHandler = async (event) => {
  const principal = await cliPrincipal(event);
  if (!principal) return json({ error: "invalid_credentials" }, { status: 401 });
  let parsed: unknown;
  try {
    parsed = await event.request.json();
  } catch {
    return json({ error: "invalid_request" }, { status: 400 });
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
    return json({ error: "invalid_request" }, { status: 400 });
  const body = parsed as Record<string, unknown>;
  if (
    typeof body.name !== "string" ||
    typeof body.edge_id !== "string" ||
    Object.keys(body).some((key) => key !== "name" && key !== "edge_id")
  )
    return json({ error: "invalid_request" }, { status: 400 });
  const result = await registerDevice(
    remoteDatabase(event),
    principal.userId,
    event.params.device_id,
    body.name,
    body.edge_id,
  );
  switch (result.status) {
    case "invalid_request":
      return json({ error: result.status }, { status: 400 });
    case "edge_not_found":
      return json({ error: result.status }, { status: 409 });
    case "conflict":
      return json({ error: "device_conflict" }, { status: 409 });
    case "created":
      return json(deviceResponseBody(result.device), { status: 201 });
    case "existing":
      return json(deviceResponseBody(result.device));
  }
};
