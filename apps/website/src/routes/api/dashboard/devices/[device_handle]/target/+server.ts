import { json } from "@sveltejs/kit";
import {
  authenticateDashboardRequest,
  dashboardPreflight,
} from "$lib/server/remote-access/dashboard-http";
import { findDashboardDeviceTarget } from "$lib/server/remote-access/device-discovery";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const OPTIONS: RequestHandler = ({ request }) => dashboardPreflight(request);

export const GET: RequestHandler = async (event) => {
  const authentication = await authenticateDashboardRequest(event);
  if ("response" in authentication) return authentication.response;
  const target = await findDashboardDeviceTarget(
    remoteDatabase(event),
    authentication.userId,
    event.params.device_handle,
  );
  if (!target)
    return json({ error: "device_not_found" }, { status: 404, headers: authentication.headers });
  return json(
    {
      device_handle: target.deviceHandle,
      device_id: target.deviceId,
      edge_api_origin: target.edgeApiOrigin,
    },
    { headers: authentication.headers },
  );
};
