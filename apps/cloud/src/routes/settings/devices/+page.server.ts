import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import { and, asc, eq } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { devices, edges } from "$lib/server/db/schema";
import type { Actions, PageServerLoad } from "./$types";

async function authenticated(event: RequestEvent) {
  const login = await currentLogin(event);
  if (!login) return null;
  return { login, db: database(environment(event).DB) };
}

export const load: PageServerLoad = async (event) => {
  const authentication = await authenticated(event);
  if (!authentication) redirect(303, "/login");
  const registeredDevices = await authentication.db
    .select({
      id: devices.id,
      name: devices.name,
      handle: devices.handle,
      edgeName: edges.name,
      createdAt: devices.createdAt,
    })
    .from(devices)
    .innerJoin(edges, eq(devices.edgeId, edges.id))
    .where(eq(devices.userId, authentication.login.user_id))
    .orderBy(asc(devices.name), asc(devices.id));
  return { devices: registeredDevices };
};

export const actions: Actions = {
  removeDevice: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: "Sign in to manage devices." });
    const value = (await event.request.formData()).get("device_id");
    if (typeof value !== "string" || !value) {
      return fail(400, { error: "Select a device to remove." });
    }
    await authentication.db
      .delete(devices)
      .where(and(eq(devices.id, value), eq(devices.userId, authentication.login.user_id)));
    return { success: "device_removed" };
  },
};
