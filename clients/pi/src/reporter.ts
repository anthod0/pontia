import { ReportQueue } from "../shared/report-queue.js";
import { appendDiagnostic } from "./diagnostics.js";
import { RpcError, type RuntimeConnection } from "../shared/control-socket.js";
import type { InternalEvent } from "./events.js";

export interface EventReportResult {
  accepted: boolean;
  conflict?: true;
  eventId?: string;
  turnId?: string;
}

export interface EventReporterOptions {
  connection: Pick<RuntimeConnection, "request">;
  logFile: string;
}

export class EventReporter {
  private readonly queue = new ReportQueue();
  private readonly connection: Pick<RuntimeConnection, "request">;
  private readonly logFile: string;

  constructor(options: EventReporterOptions) {
    this.connection = options.connection;
    this.logFile = options.logFile;
  }

  drained(): Promise<void> {
    return this.queue.drained();
  }

  async report(
    context: { runtimeId: string },
    event: InternalEvent,
    start?: Promise<EventReportResult>,
  ): Promise<EventReportResult> {
    try {
      return await this.queue.enqueue(async () => {
        if (start && !event.turn_id) {
          const { turnId } = await start;
          if (!turnId) throw new RpcError(-32602, "Pi Turn start was rejected");
          event.turn_id = turnId;
        }
        const body = await this.connection.request("event.report", {
          runtime_id: context.runtimeId,
          event,
        });
        const receipt = body as Record<string, unknown> | null;
        if (receipt?.accepted !== true)
          throw new RpcError(-32602, "Invalid Pi event acknowledgement");
        if (event.type === "turn.started" && typeof receipt.turn_id !== "string") {
          throw new RpcError(-32602, "Pi turn.started acknowledgement has no canonical turn_id");
        }
        return {
          accepted: true,
          eventId: typeof receipt.event_id === "string" ? receipt.event_id : undefined,
          turnId: typeof receipt.turn_id === "string" ? receipt.turn_id : undefined,
        };
      });
    } catch (error) {
      await appendDiagnostic(this.logFile, {
        level: "error",
        code: "pi_event_report_failed",
        message: `Discarding Pi report ${event.type}`,
        details: { event_type: event.type, report_key: event.report_key, error: String(error) },
      });
      return error instanceof RpcError && error.code === -32009
        ? { accepted: false, conflict: true }
        : { accepted: false };
    }
  }
}
