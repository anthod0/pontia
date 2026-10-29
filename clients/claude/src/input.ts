import type { SDKUserMessage } from "@anthropic-ai/claude-agent-sdk";

// Only the current user task may enter this stream. Host owns that invariant.
export class Input implements AsyncIterable<SDKUserMessage> {
  private pending?: SDKUserMessage;
  private waiter?: (value: IteratorResult<SDKUserMessage>) => void;
  private ended = false;

  push(message: SDKUserMessage): void {
    if (this.ended || this.pending) throw new Error("Input is unavailable");
    if (this.waiter) {
      const resolve = this.waiter;
      this.waiter = undefined;
      resolve({ done: false, value: message });
    } else {
      this.pending = message;
    }
  }

  end(): void {
    this.ended = true;
    this.pending = undefined;
    this.waiter?.({ done: true, value: undefined });
    this.waiter = undefined;
  }

  [Symbol.asyncIterator](): AsyncIterator<SDKUserMessage> {
    return {
      next: () => {
        if (this.pending) {
          const value = this.pending;
          this.pending = undefined;
          return Promise.resolve({ done: false, value });
        }
        if (this.ended) return Promise.resolve({ done: true, value: undefined });
        return new Promise((resolve) => {
          this.waiter = resolve;
        });
      },
    };
  }
}
