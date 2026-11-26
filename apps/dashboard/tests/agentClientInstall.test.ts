import { describe, expect, it } from "vitest";
import { lastOutputLine, readOperationStream } from "../src/features/settings/agentClientInstall";

describe("agent client installation output", () => {
  it("uses the latest non-empty carriage-return or newline segment", () => {
    expect(lastOutputLine("download 10%\rdownload 90%\n\nverified\n")).toBe("verified");
  });

  it("reads named SSE events across stream chunks", async () => {
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode('event: log\nid: 6\ndata: {"offset":0,'));
        controller.enqueue(encoder.encode('"next_offset":6,"chunk":"hello\\n"}\n\n'));
        controller.enqueue(
          encoder.encode('event: done\ndata: {"operation":{"id":"operation"},"log_size":6}\n\n'),
        );
        controller.close();
      },
    });
    const events: Array<{ event: string }> = [];

    await readOperationStream(body, (event) => events.push(event));

    expect(events).toMatchObject([
      { event: "log", offset: 0, next_offset: 6, chunk: "hello\n" },
      { event: "done", log_size: 6 },
    ]);
  });
});
