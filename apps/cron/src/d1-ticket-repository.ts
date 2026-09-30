import type {
  DeploymentOwnership,
  EdgeTicketPurpose,
  ExpiredTicket,
  TicketRepository,
} from "./cleanup";

type BooleanRow = { value: number };
type OwnershipRow = {
  current_ticket_exists: number;
  registered_edge_exists: number;
  other_deployment_ticket_exists: number;
};

export class D1TicketRepository implements TicketRepository {
  constructor(private readonly db: D1Database) {}

  async expiredTickets(expiresAtOrBefore: string) {
    const result = await this.db
      .prepare(
        `SELECT id, purpose, expected_edge_id, payload, consumed_at
         FROM edge_tickets
         WHERE expires_at <= ?1
         ORDER BY expires_at, id`,
      )
      .bind(expiresAtOrBefore)
      .all<{
        id: number;
        purpose: EdgeTicketPurpose;
        expected_edge_id: string;
        payload: string;
        consumed_at: string | null;
      }>();
    return result.results.map((row): ExpiredTicket => ({
      id: row.id,
      purpose: row.purpose,
      expectedEdgeId: row.expected_edge_id,
      payload: row.payload,
      consumedAt: row.consumed_at,
    }));
  }

  async registeredEdgeExists(edgeId: string) {
    const row = await this.db
      .prepare("SELECT EXISTS(SELECT 1 FROM edges WHERE id = ?1) AS value")
      .bind(edgeId)
      .first<BooleanRow>();
    return row?.value === 1;
  }

  async deploymentOwnership(
    ticket: ExpiredTicket,
    name: string,
    expiresAtOrBefore: string,
  ): Promise<DeploymentOwnership> {
    const row = await this.db
      .prepare(
        `SELECT
           EXISTS(
             SELECT 1 FROM edge_tickets
             WHERE id = ?1 AND purpose = 'edge_deployment' AND expires_at <= ?2
           ) AS current_ticket_exists,
           EXISTS(
             SELECT 1 FROM edges
             WHERE id = ?3 OR name = ?4
           ) AS registered_edge_exists,
           EXISTS(
             SELECT 1 FROM edge_tickets
             WHERE id <> ?1
               AND purpose = 'edge_deployment'
               AND json_extract(payload, '$.name') = ?4
           ) AS other_deployment_ticket_exists`,
      )
      .bind(ticket.id, expiresAtOrBefore, ticket.expectedEdgeId, name)
      .first<OwnershipRow>();
    if (row === null) throw new Error("ownership_query_failed");
    return {
      currentTicketExists: row.current_ticket_exists === 1,
      registeredEdgeExists: row.registered_edge_exists === 1,
      otherDeploymentTicketExists: row.other_deployment_ticket_exists === 1,
    };
  }

  async deleteExpiredTicket(ticketId: number, expiresAtOrBefore: string) {
    const result = await this.db
      .prepare("DELETE FROM edge_tickets WHERE id = ?1 AND expires_at <= ?2")
      .bind(ticketId, expiresAtOrBefore)
      .run();
    return (result.meta.changes ?? 0) === 1;
  }
}
