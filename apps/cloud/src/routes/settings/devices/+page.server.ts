import { fail, redirect, type RequestEvent } from "@sveltejs/kit";
import { and, asc, eq } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { m } from "$lib/paraglide/messages";
import { localizeHref } from "$lib/paraglide/runtime";
import { devices, edges } from "$lib/server/db/schema";
import { isValidDeviceHandle } from "$lib/server/remote-access/device-handle";
import type { Actions, PageServerLoad } from "./$types";

async function authenticated(event: RequestEvent) {
  const login = await currentLogin(event);
  if (!login) return null;
  return { login, db: database(environment(event).DB) };
}

export const load: PageServerLoad = async (event) => {
  const authentication = await authenticated(event);
  if (!authentication) redirect(303, localizeHref("/login"));
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
  updateDevice: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: m.devices_error_sign_in() });
    const form = await event.request.formData();
    const deviceId = form.get("device_id");
    const nameValue = form.get("name");
    const handleValue = form.get("handle");
    if (typeof deviceId !== "string" || !deviceId) {
      return fail(400, { error: m.devices_error_select_edit() });
    }
    if (typeof nameValue !== "string") {
      return fail(400, { error: m.devices_error_name_required() });
    }
    if (typeof handleValue !== "string") {
      return fail(400, { error: m.devices_error_handle_required() });
    }
    const name = nameValue.trim();
    const handle = handleValue.trim();
    if (!name || /[\u0000-\u001f\u007f-\u009f]/.test(name)) {
      return fail(400, { error: m.devices_error_name_invalid() });
    }
    if (!isValidDeviceHandle(handle)) {
      return fail(400, {
        error: m.devices_error_handle_invalid(),
      });
    }

    const handleOwner = await authentication.db
      .select({ id: devices.id })
      .from(devices)
      .where(and(eq(devices.userId, authentication.login.user_id), eq(devices.handle, handle)))
      .get();
    if (handleOwner && handleOwner.id !== deviceId) {
      return fail(409, { error: m.devices_error_handle_taken() });
    }

    let updated: { id: string }[];
    try {
      updated = await authentication.db
        .update(devices)
        .set({ name, handle, updatedAt: new Date().toISOString() })
        .where(and(eq(devices.id, deviceId), eq(devices.userId, authentication.login.user_id)))
        .returning({ id: devices.id });
    } catch (cause) {
      const conflictingDevice = await authentication.db
        .select({ id: devices.id })
        .from(devices)
        .where(and(eq(devices.userId, authentication.login.user_id), eq(devices.handle, handle)))
        .get();
      if (conflictingDevice && conflictingDevice.id !== deviceId) {
        return fail(409, { error: m.devices_error_handle_taken() });
      }
      throw cause;
    }
    if (updated.length === 0) return fail(404, { error: m.devices_error_not_found() });
    return { success: "device_updated" };
  },

  removeDevice: async (event) => {
    const authentication = await authenticated(event);
    if (!authentication) return fail(401, { error: m.devices_error_sign_in() });
    const value = (await event.request.formData()).get("device_id");
    if (typeof value !== "string" || !value) {
      return fail(400, { error: m.devices_error_select_remove() });
    }
    const userId = authentication.login.user_id;
    await authentication.db
      .delete(devices)
      .where(and(eq(devices.id, value), eq(devices.userId, userId)));
    return { success: "device_removed" };
  },
};
