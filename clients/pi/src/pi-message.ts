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

export function errorMessageFromAgentEnd(event: unknown): string | undefined {
  if (!event || typeof event !== "object") return undefined;
  const record = event as Record<string, unknown>;
  const error = record.error;
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.length > 0) return error;
  if (record.isError === true) return "pi agent reported an error";
  return undefined;
}

export function lastAssistantTextFromMessages(messages: unknown): string | undefined {
  if (!Array.isArray(messages)) return undefined;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const text = assistantTextFromMessage(messages[index]);
    if (text) return text;
  }
  return undefined;
}
