function textFromContent(content: unknown): string | undefined {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return undefined;

  const text = content
    .map((part) => {
      if (!part || typeof part !== "object") return "";
      const record = part as Record<string, unknown>;
      return record.type === "text" && typeof record.text === "string" ? record.text : "";
    })
    .join("");

  return text.length > 0 ? text : undefined;
}

export function assistantTextFromMessage(message: unknown): string | undefined {
  if (!message || typeof message !== "object") return undefined;
  const record = message as Record<string, unknown>;
  if (record.role !== "assistant") return undefined;
  return textFromContent(record.content);
}

export function assistantDeltaFromEvent(event: unknown): string | undefined {
  if (!event || typeof event !== "object") return undefined;
  const record = event as Record<string, unknown>;
  const streamEvent = record.assistantMessageEvent;
  if (streamEvent && typeof streamEvent === "object") {
    const streamRecord = streamEvent as Record<string, unknown>;
    for (const key of ["text_delta", "textDelta", "delta", "text"] as const) {
      const value = streamRecord[key];
      if (typeof value === "string" && value.length > 0) return value;
    }
  }
  return undefined;
}

const transcriptBoundaryStreamEventTypes = new Set([
  "thinking_start",
  "thinking_end",
  "text_start",
  "text_end",
  "toolcall_start",
  "toolcall_end",
]);

export function isTranscriptBoundaryMessageUpdate(event: unknown): boolean {
  if (!event || typeof event !== "object") return false;
  const streamEvent = (event as Record<string, unknown>).assistantMessageEvent;
  if (!streamEvent || typeof streamEvent !== "object") return false;
  const type = (streamEvent as Record<string, unknown>).type;
  return typeof type === "string" && transcriptBoundaryStreamEventTypes.has(type);
}

interface PiAgentEndEventLike {
  messages: unknown[];
}

function lastAssistantMessageFromAgentEnd(event: PiAgentEndEventLike): Record<string, unknown> | undefined {
  for (let index = event.messages.length - 1; index >= 0; index -= 1) {
    const message = event.messages[index];
    if (message && typeof message === "object" && (message as Record<string, unknown>).role === "assistant") {
      return message as Record<string, unknown>;
    }
  }
  return undefined;
}

export function agentEndWasInterrupted(event: PiAgentEndEventLike): boolean {
  return lastAssistantMessageFromAgentEnd(event)?.stopReason === "aborted";
}

export function errorMessageFromAgentEnd(event: PiAgentEndEventLike): string | undefined {
  const assistant = lastAssistantMessageFromAgentEnd(event);
  if (assistant?.stopReason !== "error") return undefined;
  return typeof assistant.errorMessage === "string" && assistant.errorMessage.length > 0
    ? assistant.errorMessage
    : "pi agent reported an error";
}

export function lastAssistantTextFromMessages(messages: unknown): string | undefined {
  if (!Array.isArray(messages)) return undefined;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const text = assistantTextFromMessage(messages[index]);
    if (text) return text;
  }
  return undefined;
}
