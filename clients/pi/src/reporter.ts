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

  async report(context: { runtimeInstanceId: string }, event: InternalEvent): Promise<EventReportResult> {
    try {
      const body = await this.connection.request("event.report", {
        runtime_instance_id: context.runtimeInstanceId,
        event,
      });
      const record = body && typeof body === "object" ? body as Record<string, unknown> : undefined;
      if (event.type === "turn.started" && (record?.accepted !== true || typeof record?.turn_id !== "string" || !record.turn_id)) {
        await this.reportStartFailure(event, "missing_turn_id");
        return { accepted: false };
      }
      if (record?.accepted !== true) throw new Error("Pi event RPC returned an invalid acknowledgement");
      return {
        accepted: true,
        eventId: typeof record.event_id === "string" ? record.event_id : undefined,
        turnId: typeof record.turn_id === "string" ? record.turn_id : undefined,
      };
    } catch (error) {
      await appendDiagnostic(this.logFile, {
        level: "error",
        code: "pi_event_report_failed",
        message: `Pi event RPC failed for ${event.type}`,
        details: { event_type: event.type, error: error instanceof Error ? error.message : String(error) },
      });
      await this.reportStartFailure(event, error instanceof RpcError ? "event_rejected" : "transport_failed");
      return { accepted: false };
    }
  }

  private async reportStartFailure(
    event: InternalEvent,
    reason: "event_rejected" | "transport_failed" | "missing_turn_id",
  ): Promise<void> {
    if (event.type !== "turn.started" || typeof event.data.runtime_instance_id !== "string") return;
    // A lost turn.started response is ambiguous: retrying it could create a
    // second Turn. Only this fenced, idempotent failure notification is retried.
    for (let attempt = 0; attempt < 3; attempt++) {
      if (attempt > 0) await new Promise((resolve) => setTimeout(resolve, 100 * attempt));
      try {
        const result = await this.connection.request("turn.startFailure", {
          session_id: event.session_id, runtime_instance_id: event.data.runtime_instance_id, reason,
        }) as { accepted?: unknown } | null;
        if (result?.accepted === true) return;
      } catch (error) {
        if (error instanceof RpcError && error.code !== -32603) break;
        // Exhaustion is visible locally; an unreachable server cannot persist
        // a state transition. Never claim that this notification succeeded.
      }
    }
    await appendDiagnostic(this.logFile, {
      level: "error",
      code: "turn_start_failure_report_failed",
      message: "could not persist turn.started reporting failure; workflow state is unconfirmed",
      details: { session_id: event.session_id, reason },
    });
  }
}
