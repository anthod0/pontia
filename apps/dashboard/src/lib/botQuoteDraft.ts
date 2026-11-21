import type { BotQuote } from "./botQuote";

// The quote waiting above a Bot chat composer survives reloads until it is sent or removed.
function key(deviceId: string, botId: string): string {
  return `pontia:bot-quote:v1:${JSON.stringify([deviceId, botId])}`;
}

export function loadQuoteDraft(deviceId: string, botId: string): BotQuote | null {
  const saved = localStorage.getItem(key(deviceId, botId));
  return saved ? (JSON.parse(saved) as BotQuote) : null;
}

export function saveQuoteDraft(deviceId: string, botId: string, quote: BotQuote | null): void {
  if (quote) localStorage.setItem(key(deviceId, botId), JSON.stringify(quote));
  else localStorage.removeItem(key(deviceId, botId));
}
