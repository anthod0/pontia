import { appendDiagnostic } from "./diagnostics.js";
import type { InternalEvent } from "./events.js";

export interface EventReportResult {
  accepted: boolean;
  turnId?: string;
}

export interface EventReporterOptions {
  fetch?: typeof fetch;
  logFile: string;
}

export class EventReporter {
  private readonly fetchImpl: typeof fetch;
  private readonly logFile: string;

  constructor(options: EventReporterOptions) {
    this.fetchImpl = options.fetch ?? fetch;
    this.logFile = options.logFile;
  }

  async report(context: { internalEventUrl: string }, event: InternalEvent): Promise<EventReportResult> {
    try {
      const response = await this.fetchImpl(context.internalEventUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(event),
      });
      const body = await response.text().catch(() => "");
      if (!response.ok) {
        await appendDiagnostic(this.logFile, {
          level: "error",
          code: "internal_event_post_failed",
          message: `Internal Event API rejected ${event.type}: ${response.status} ${response.statusText}`,
          details: { event_type: event.type, status: response.status, body },
        });
        return { accepted: false };
      }
      let responseBody: unknown;
      try {
        responseBody = body ? JSON.parse(body) : null;
      } catch {
        responseBody = null;
      }
      const record = responseBody && typeof responseBody === "object" ? responseBody as Record<string, unknown> : undefined;
      return {
        accepted: true,
        turnId: typeof record?.turn_id === "string" ? record.turn_id : undefined,
      };
    } catch (error) {
      await appendDiagnostic(this.logFile, {
        level: "error",
        code: "internal_event_post_exception",
        message: `Internal Event API POST failed for ${event.type}`,
        details: { event_type: event.type, error: error instanceof Error ? error.message : String(error) },
      });
      return { accepted: false };
    }
  }
}
