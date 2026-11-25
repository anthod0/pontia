export type ExpiredTicket = { id: number };

export interface TicketRepository {
  expiredTickets(expiresAtOrBefore: string): Promise<ExpiredTicket[]>;
  deleteExpiredTicket(ticketId: number, expiresAtOrBefore: string): Promise<boolean>;
}

export interface CleanupLogger {
  info(value: Record<string, unknown>): void;
  error(value: Record<string, unknown>): void;
}

export type CleanupSummary = {
  scanned: number;
  ticketsDeleted: number;
  skipped: number;
  failed: number;
};

function log(logger: CleanupLogger, level: "info" | "error", value: Record<string, unknown>) {
  try {
    logger[level](value);
  } catch {
    // Cleanup results must not depend on diagnostic output.
  }
}

export async function cleanupExpiredEdgeTickets(
  now: Date,
  repository: TicketRepository,
  logger: CleanupLogger,
): Promise<CleanupSummary> {
  const deadline = now.toISOString();
  const tickets = await repository.expiredTickets(deadline);
  const summary: CleanupSummary = {
    scanned: tickets.length,
    ticketsDeleted: 0,
    skipped: 0,
    failed: 0,
  };

  for (const ticket of tickets) {
    try {
      if (await repository.deleteExpiredTicket(ticket.id, deadline)) summary.ticketsDeleted += 1;
      else summary.skipped += 1;
    } catch {
      summary.failed += 1;
      log(logger, "error", {
        event: "edge_ticket_cleanup_item_failed",
        ticket_id: ticket.id,
        error: "unexpected_error",
      });
    }
  }

  log(logger, "info", { event: "edge_ticket_cleanup_finished", ...summary });
  return summary;
}
