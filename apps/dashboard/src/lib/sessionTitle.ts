import { translate as defaultTranslate, type Translator } from "$lib/i18n";
import { clientDisplayName } from "./clientName";

export function untitledSessionLabel(
  clientType: string | null | undefined,
  translate: Translator = defaultTranslate,
): string {
  const normalized = clientType?.trim();
  const name = normalized === "claude-agent" ? "Claude Code" : clientDisplayName(normalized ?? "");
  return `Untitled ${name || translate("chat.agent")} session`;
}
