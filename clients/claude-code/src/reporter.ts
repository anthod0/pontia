import { appendDiagnostic } from "./diagnostics.js";
import type { InternalEvent } from "./events.js";

type FetchLike = typeof fetch;

export interface EventReporterOptions {
  fetch?: FetchLike;
  logFile: string;
}

export class EventReporter {
  private readonly fetchImpl: FetchLike;
  private readonly logFile: string;

  constructor(options: EventReporterOptions) {
    this.fetchImpl = options.fetch ?? fetch;
    this.logFile = options.logFile;
  }

  async report(context: { internalEventUrl: string }, event: InternalEvent): Promise<boolean> {
    try {
      const response = await this.fetchImpl(context.internalEventUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(event),
      });

      if (!response.ok) {
        const body = await response.text().catch(() => "");
        await appendDiagnostic(this.logFile, {
          level: "error",
          code: "internal_event_post_failed",
          message: `Internal Event API rejected ${event.type}: ${response.status} ${response.statusText}`,
          details: { event_id: event.event_id, status: response.status, body },
        });
        return false;
      }
      return true;
    } catch (error) {
      await appendDiagnostic(this.logFile, {
        level: "error",
        code: "internal_event_post_exception",
        message: `Internal Event API POST failed for ${event.type}`,
        details: { event_id: event.event_id, error: error instanceof Error ? error.message : String(error) },
      });
      return false;
    }
  }
}
