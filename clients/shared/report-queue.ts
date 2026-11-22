import { setTimeout } from "node:timers/promises";
import { AttachmentRejected, RpcError } from "./control-socket.js";

// A queue belongs to one runtime attachment. Its owner captures the fact and key
// before enqueueing so every retry sends the same observation.
export class ReportQueue {
  private tail: Promise<void> = Promise.resolve();
  private rejected?: AttachmentRejected;

  drained(): Promise<void> {
    return this.tail;
  }

  enqueue<T>(send: () => Promise<T>): Promise<T> {
    const result = this.tail.then(async () => {
      let delay = 250;
      for (;;) {
        if (this.rejected) throw this.rejected;
        try {
          return await send();
        } catch (error) {
          if (error instanceof AttachmentRejected) this.rejected = error;
          if (this.rejected || (error instanceof RpcError && error.code !== -32603)) throw error;
          await setTimeout(delay, undefined, { ref: false });
          delay = Math.min(delay * 2, 5000);
        }
      }
    });
    this.tail = result.then(
      () => {},
      () => {},
    );
    return result;
  }
}
