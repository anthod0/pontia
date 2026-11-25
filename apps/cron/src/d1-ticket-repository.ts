import type { ExpiredTicket, TicketRepository } from "./cleanup";

export class D1TicketRepository implements TicketRepository {
  constructor(private readonly db: D1Database) {}

  async expiredTickets(expiresAtOrBefore: string) {
    const result = await this.db
      .prepare(
        `SELECT id
         FROM edge_tickets
         WHERE expires_at <= ?1
         ORDER BY expires_at, id`,
      )
      .bind(expiresAtOrBefore)
      .all<{ id: number }>();
    return result.results.map((row): ExpiredTicket => ({ id: row.id }));
  }

  async deleteExpiredTicket(ticketId: number, expiresAtOrBefore: string) {
    const result = await this.db
      .prepare("DELETE FROM edge_tickets WHERE id = ?1 AND expires_at <= ?2")
      .bind(ticketId, expiresAtOrBefore)
      .run();
    return (result.meta.changes ?? 0) === 1;
  }
}
