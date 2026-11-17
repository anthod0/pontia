import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import type { SessionView } from "../api/types";

export function modelPickerDisabledReason(
  session: SessionView,
  translate: Translator = defaultTranslate,
): string | undefined {
  if (session.capabilities.list_models !== true) return translate("model.listUnavailable");
  if (!["idle", "busy", "waiting_for_interaction"].includes(session.state))
    return translate("model.sessionNotRunning");
  return session.model_control_unavailable_reason ?? undefined;
}
