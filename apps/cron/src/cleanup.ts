import { DnsProviderError } from "./dns-errors";

const EDGE_ZONE = "edge.pontia.dev";
const DNS_LABEL = /^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$/;

export type EdgeTicketPurpose = "edge_deployment" | "device_tunnel" | "dashboard_access";

export type ExpiredTicket = {
  id: number;
  purpose: EdgeTicketPurpose;
  expectedEdgeId: string;
  payload: string;
  consumedAt: string | null;
};

export type DeploymentOwnership = {
  currentTicketExists: boolean;
  registeredEdgeExists: boolean;
  otherDeploymentTicketExists: boolean;
};

export interface TicketRepository {
  expiredTickets(expiresAtOrBefore: string): Promise<ExpiredTicket[]>;
  registeredEdgeExists(edgeId: string): Promise<boolean>;
  deploymentOwnership(
    ticket: ExpiredTicket,
    name: string,
    expiresAtOrBefore: string,
  ): Promise<DeploymentOwnership>;
  deleteExpiredTicket(ticketId: number, expiresAtOrBefore: string): Promise<boolean>;
}

export type DnsARecord = { id: string; hostname: string };

export interface DnsProvider {
  findA(hostname: string): Promise<DnsARecord | null>;
  deleteA(record: DnsARecord): Promise<void>;
}

export interface CleanupLogger {
  info(value: Record<string, unknown>): void;
  error(value: Record<string, unknown>): void;
}

export type CleanupSummary = {
  scanned: number;
  ticketsDeleted: number;
  dnsRecordsDeleted: number;
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

function deploymentName(payload: string): string | null {
  let value: unknown;
  try {
    value = JSON.parse(payload);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const keys = Object.keys(value);
  if (keys.length !== 1 || keys[0] !== "name") return null;
  const name = (value as Record<string, unknown>).name;
  return typeof name === "string" && name.length <= 63 && DNS_LABEL.test(name) ? name : null;
}

function hostnameFor(name: string) {
  return `${name}.${EDGE_ZONE}`;
}

async function deleteTicket(
  repository: TicketRepository,
  ticketId: number,
  deadline: string,
  summary: CleanupSummary,
) {
  if (await repository.deleteExpiredTicket(ticketId, deadline)) summary.ticketsDeleted += 1;
  else summary.skipped += 1;
}

async function deploymentIsUnowned(
  ownership: DeploymentOwnership,
  ticket: ExpiredTicket,
  deadline: string,
  repository: TicketRepository,
  summary: CleanupSummary,
) {
  if (!ownership.currentTicketExists || ownership.otherDeploymentTicketExists) {
    summary.skipped += 1;
    return false;
  }
  if (ownership.registeredEdgeExists) {
    await deleteTicket(repository, ticket.id, deadline, summary);
    return false;
  }
  return true;
}

async function cleanupDeployment(
  ticket: ExpiredTicket,
  deadline: string,
  repository: TicketRepository,
  dns: DnsProvider,
  summary: CleanupSummary,
) {
  if (
    ticket.consumedAt !== null ||
    (await repository.registeredEdgeExists(ticket.expectedEdgeId))
  ) {
    await deleteTicket(repository, ticket.id, deadline, summary);
    return;
  }

  const name = deploymentName(ticket.payload);
  if (name === null) throw new Error("invalid_deployment_payload");

  const initialOwnership = await repository.deploymentOwnership(ticket, name, deadline);
  if (!(await deploymentIsUnowned(initialOwnership, ticket, deadline, repository, summary))) return;

  const record = await dns.findA(hostnameFor(name));
  const ownership = await repository.deploymentOwnership(ticket, name, deadline);
  if (!(await deploymentIsUnowned(ownership, ticket, deadline, repository, summary))) return;

  if (record !== null) {
    await dns.deleteA(record);
    summary.dnsRecordsDeleted += 1;
  }
  await deleteTicket(repository, ticket.id, deadline, summary);
}

export async function cleanupExpiredEdgeTickets(
  now: Date,
  repository: TicketRepository,
  dns: DnsProvider,
  logger: CleanupLogger,
): Promise<CleanupSummary> {
  const deadline = now.toISOString();
  const tickets = await repository.expiredTickets(deadline);
  const summary: CleanupSummary = {
    scanned: tickets.length,
    ticketsDeleted: 0,
    dnsRecordsDeleted: 0,
    skipped: 0,
    failed: 0,
  };
  const ordered = tickets.toSorted((left, right) => {
    const leftDeployment = left.purpose === "edge_deployment" ? 1 : 0;
    const rightDeployment = right.purpose === "edge_deployment" ? 1 : 0;
    return leftDeployment - rightDeployment || left.id - right.id;
  });

  for (const ticket of ordered) {
    try {
      if (ticket.purpose === "edge_deployment") {
        await cleanupDeployment(ticket, deadline, repository, dns, summary);
      } else {
        await deleteTicket(repository, ticket.id, deadline, summary);
      }
    } catch (error) {
      summary.failed += 1;
      const name = ticket.purpose === "edge_deployment" ? deploymentName(ticket.payload) : null;
      log(logger, "error", {
        event: "edge_ticket_cleanup_item_failed",
        ticket_id: ticket.id,
        ...(name === null ? {} : { hostname: hostnameFor(name) }),
        ...(error instanceof DnsProviderError
          ? error.fields()
          : {
              error:
                error instanceof Error && error.message === "invalid_deployment_payload"
                  ? error.message
                  : "unexpected_error",
            }),
      });
    }
  }

  log(logger, "info", { event: "edge_ticket_cleanup_finished", ...summary });
  return summary;
}
