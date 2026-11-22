import { limitQuote, type BotQuote } from "./botQuote";
import { PersistedState, scopedKey } from "./storage";

// The quote waiting above a Bot chat composer survives reloads until it is sent or removed.
export function createQuoteDraft(deviceId: string, botId: string): PersistedState<BotQuote | null> {
  return new PersistedState<BotQuote | null>(
    scopedKey("pontia:bot-quote:v2", deviceId, botId),
    null,
    {
      parse: (saved) => limitQuote(JSON.parse(saved) as BotQuote),
      stringify: (quote) => (quote ? JSON.stringify(quote) : null),
    },
  );
}
