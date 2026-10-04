import { json } from "@sveltejs/kit";
import { cliPrincipal, remoteDatabase } from "$lib/server/remote-access/http";
import { verifyDeviceKeyProof } from "$lib/server/remote-access/device-key-proof";
import {
  findRegisteredDevice,
  registerDevice,
  unregisterDevice,
} from "$lib/server/remote-access/registration";
import type { RequestHandler } from "./$types";

function deviceResponseBody(
  device: {
    id: string;
    handle: string;
    name: string;
    edgeId: string;
    edgeName: string;
    e2ePublicKey: string;
    e2eKeyVersion: number;
  },
  capabilityVerificationKey: string,
) {
  return {
    id: device.id,
    device_handle: device.handle,
    name: device.name,
    edge_id: device.edgeId,
    edge_name: device.edgeName,
    e2e_public_key: device.e2ePublicKey,
    e2e_key_version: device.e2eKeyVersion,
    capability_verification_key: capabilityVerificationKey,
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
  const trustKey = event.platform?.env.E2E_CAPABILITY_VERIFICATION_KEY;
  if (!trustKey) return json({ error: "unavailable" }, { status: 503 });
  return json(deviceResponseBody(result.device, trustKey));
};

export const DELETE: RequestHandler = async (event) => {
  const principal = await cliPrincipal(event);
  if (!principal) return json({ error: "invalid_credentials" }, { status: 401 });
  const result = await unregisterDevice(
    remoteDatabase(event),
    principal.userId,
    event.params.device_id,
  );
  if (result.status === "conflict") return json({ error: "device_conflict" }, { status: 409 });
  return new Response(null, { status: 204 });
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
    typeof body.e2e_public_key !== "string" ||
    typeof body.e2e_key_version !== "number" ||
    typeof body.e2e_key_proof !== "string" ||
    Object.keys(body).some(
      (key) =>
        !["name", "edge_id", "e2e_public_key", "e2e_key_version", "e2e_key_proof"].includes(key),
    )
  )
    return json({ error: "invalid_request" }, { status: 400 });
  const proofPrivateKey = event.platform?.env.E2E_REGISTRATION_PROOF_PRIVATE_KEY;
  if (!proofPrivateKey) return json({ error: "unavailable" }, { status: 503 });
  if (
    !(await verifyDeviceKeyProof(
      event.params.device_id,
      body.e2e_key_version,
      body.e2e_public_key,
      body.e2e_key_proof,
      proofPrivateKey,
    ))
  )
    return json({ error: "invalid_key_proof" }, { status: 400 });
  const result = await registerDevice(
    remoteDatabase(event),
    principal.userId,
    event.params.device_id,
    body.name,
    body.edge_id,
    body.e2e_public_key,
    body.e2e_key_version,
  );
  const trustKey = event.platform?.env.E2E_CAPABILITY_VERIFICATION_KEY;
  if (!trustKey) return json({ error: "unavailable" }, { status: 503 });
  switch (result.status) {
    case "invalid_request":
      return json({ error: result.status }, { status: 400 });
    case "edge_not_found":
      return json({ error: result.status }, { status: 409 });
    case "conflict":
      return json({ error: "device_conflict" }, { status: 409 });
    case "created":
      return json(deviceResponseBody(result.device, trustKey), { status: 201 });
    case "existing":
      return json(deviceResponseBody(result.device, trustKey));
  }
};
