import { redirect } from "@sveltejs/kit";
import { settingsRedirectPath } from "$dashboard-mode/settingsRedirect";

export function load({ params }: { params: Record<string, string | undefined> }): never {
  redirect(307, settingsRedirectPath(params.handle));
}
