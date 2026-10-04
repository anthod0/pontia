import { json } from "@sveltejs/kit";
import { cliPrincipal } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  if (!(await cliPrincipal(event))) return json({ error: "invalid_credentials" }, { status: 401 });
  const proofKey = event.platform?.env.E2E_REGISTRATION_PROOF_PUBLIC_KEY;
  const verificationKey = event.platform?.env.E2E_CAPABILITY_VERIFICATION_KEY;
  if (!proofKey || !verificationKey) return json({ error: "unavailable" }, { status: 503 });
  return json({
    registration_proof_public_key: proofKey,
    capability_verification_key: verificationKey,
  });
};
