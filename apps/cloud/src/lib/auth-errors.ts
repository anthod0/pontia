import { m } from "$lib/paraglide/messages";

const messages: Record<string, () => string> = {
  invalid_credentials: m.auth_error_invalid_credentials,
  invalid_oauth: m.auth_error_invalid_oauth,
  account_conflict: m.auth_error_account_conflict,
  provider_failed: m.auth_error_provider_failed,
};

export function authError(code: string | null) {
  if (!code) return null;
  return (Object.hasOwn(messages, code) ? messages[code] : m.auth_error_unknown)();
}
