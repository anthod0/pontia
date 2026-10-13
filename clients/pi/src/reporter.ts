import { appendDiagnostic } from "./diagnostics.js";
import { RpcError, type PiConnection } from "./control-socket.js";
import type { InternalEvent } from "./events.js";

export interface EventReportResult {
  accepted: boolean;
  eventId?: string;
  turnId?: string;
}

export interface EventReporterOptions {
  connection: Pick<PiConnection, "request">;
  logFile: string;
}

export class EventReporter {
  private readonly connection: Pick<PiConnection, "request">;
  private readonly logFile: string;

  constructor(options: EventReporterOptions) {
    this.connection = options.connection;
    this.logFile = options.logFile;
  }

  async report(context: { runtimeId: string }, event: InternalEvent): Promise<EventReportResult> {
    const attempts = event.type === "turn.started" ? 3 : 1;
    let lastError: unknown;

    for (let attempt = 0; attempt < attempts; attempt++) {
      if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, 100 * attempt));
      try {
        const body = await this.connection.request("event.report", {
          runtime_id: context.runtimeId,
          event,
        });
        const record =
          body && typeof body === "object" ? (body as Record<string, unknown>) : undefined;
        if (record?.accepted !== true) {
          if (record?.accepted === false) {
            lastError = new Error("Pi event report was rejected");
            break;
          }
          throw new Error("Pi event RPC returned an invalid acknowledgement");
        }
        if (
          event.type === "turn.started" &&
          (typeof record.turn_id !== "string" || !record.turn_id)
        ) {
          throw new Error("Pi turn.started acknowledgement has no canonical turn_id");
        }
        return {
          accepted: true,
          eventId: typeof record.event_id === "string" ? record.event_id : undefined,
          turnId: typeof record.turn_id === "string" ? record.turn_id : undefined,
        };
      } catch (error) {
        lastError = error;
        if (error instanceof RpcError && error.code !== -32603) break;
      }
    }

    await appendDiagnostic(this.logFile, {
      level: "error",
      code: "pi_event_report_failed",
      message: `Pi event RPC failed for ${event.type}`,
      details: {
        event_type: event.type,
        report_key: event.report_key,
        error: lastError instanceof Error ? lastError.message : String(lastError),
      },
    });
    return { accepted: false };
  }
}
