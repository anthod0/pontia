import { json } from "@sveltejs/kit";
import {
  authenticateDashboardRequest,
  dashboardPreflight,
} from "$lib/server/remote-access/dashboard-http";
import { listDashboardDevices } from "$lib/server/remote-access/device-discovery";
import { remoteDatabase } from "$lib/server/remote-access/http";
import type { RequestHandler } from "./$types";

export const OPTIONS: RequestHandler = ({ request }) => dashboardPreflight(request);

export const GET: RequestHandler = async (event) => {
  const authentication = await authenticateDashboardRequest(event);
  if ("response" in authentication) return authentication.response;
  const devices = await listDashboardDevices(remoteDatabase(event), authentication.userId);
  return json(
    {
      user_id: authentication.userId,
      devices: devices.map((device) => ({
        device_handle: device.deviceHandle,
        device_id: device.deviceId,
        edge_id: device.edgeId,
        name: device.name,
      })),
    },
    { headers: authentication.headers },
  );
};
