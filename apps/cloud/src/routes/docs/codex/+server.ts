import { redirect } from "@sveltejs/kit";
import { localizeHref } from "$lib/paraglide/runtime";
import type { RequestHandler } from "./$types";

export const prerender = false;

export const GET: RequestHandler = ({ locals }) =>
  redirect(301, localizeHref("/docs/agent-clients/codex", { locale: locals.locale }));
