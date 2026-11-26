export async function readSse(
  body: ReadableStream<Uint8Array>,
  onData: (data: string) => void,
): Promise<void> {
  return readSseFrames(body, ({ data }) => onData(data));
}

export interface SseFrame {
  event: string;
  id: string | null;
  data: string;
}

export async function readSseFrames(
  body: ReadableStream<Uint8Array>,
  onFrame: (frame: SseFrame) => void,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let boundary = buffer.search(/\r?\n\r?\n/);
      while (boundary !== -1) {
        const frame = buffer.slice(0, boundary);
        buffer = buffer.slice(buffer[boundary] === "\r" ? boundary + 4 : boundary + 2);
        const lines = frame.split(/\r?\n/);
        const data = lines
          .filter((line) => line.startsWith("data:"))
          .map((line) => line.slice(5).trimStart())
          .join("\n");
        if (data)
          onFrame({
            event:
              lines
                .find((line) => line.startsWith("event:"))
                ?.slice(6)
                .trim() ?? "message",
            id:
              lines
                .find((line) => line.startsWith("id:"))
                ?.slice(3)
                .trim() ?? null,
            data,
          });
        boundary = buffer.search(/\r?\n\r?\n/);
      }
    }
  } finally {
    reader.releaseLock();
  }
}
