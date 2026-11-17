import type { SDKUserMessage } from "@anthropic-ai/claude-agent-sdk";

// Host admits a task and its steering messages to the SDK stream.
export class Input implements AsyncIterable<SDKUserMessage> {
  private pending: SDKUserMessage[] = [];
  private waiter?: (value: IteratorResult<SDKUserMessage>) => void;
  private ended = false;

  push(message: SDKUserMessage): void {
    if (this.ended) throw new Error("Input is unavailable");
    if (this.waiter) {
      const resolve = this.waiter;
      this.waiter = undefined;
      resolve({ done: false, value: message });
    } else {
      this.pending.push(message);
    }
  }

  end(): void {
    this.ended = true;
    this.pending = [];
    this.waiter?.({ done: true, value: undefined });
    this.waiter = undefined;
  }

  [Symbol.asyncIterator](): AsyncIterator<SDKUserMessage> {
    return {
      next: () => {
        const value = this.pending.shift();
        if (value) {
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
