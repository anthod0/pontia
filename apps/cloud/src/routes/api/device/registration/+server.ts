import { json } from "@sveltejs/kit";
import { and, eq, gte } from "drizzle-orm";
import { currentLogin, environment } from "$lib/server/auth/http";
import { database } from "$lib/server/db";
import { devices } from "$lib/server/db/schema";
import { isUuidV7 } from "$lib/server/uuid";
import type { RequestHandler } from "./$types";

export const GET: RequestHandler = async (event) => {
  const login = await currentLogin(event);
  if (!login) return new Response(null, { status: 401 });
  const id = event.url.searchParams.get("init_device") ?? "";
  const since = new Date(Number(event.url.searchParams.get("approved_at")));
  if (!isUuidV7(id) || !(since.getTime() > 0)) return new Response(null, { status: 400 });
  const device = await database(environment(event).DB)
    .select({ handle: devices.handle })
    .from(devices)
    .where(
      and(
        eq(devices.id, id),
        eq(devices.userId, login.user_id),
        gte(devices.createdAt, since.toISOString()),
      ),
    )
    .get();
  return json({ handle: device?.handle ?? null }, { headers: { "Cache-Control": "no-store" } });
};
