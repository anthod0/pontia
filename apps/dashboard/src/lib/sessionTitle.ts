import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import { clientDisplayName, dashboardClient } from "../clients";

export function untitledSessionLabel(
  clientType: string | null | undefined,
  translate: Translator = defaultTranslate,
): string {
  const normalized = clientType?.trim();
  const name = dashboardClient(normalized)?.titleLabel ?? clientDisplayName(normalized ?? "");
  return `Untitled ${name || translate("chat.agent")} session`;
}
