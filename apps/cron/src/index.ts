import {
  cleanupExpiredEdgeTickets,
  type CleanupLogger,
  type DnsProvider,
  type TicketRepository,
} from "./cleanup";
import { CloudflareDnsProvider } from "./cloudflare-dns";
import { D1TicketRepository } from "./d1-ticket-repository";

export interface Env {
  DB: D1Database;
  CLOUDFLARE_DNS_TOKEN: string;
  CLOUDFLARE_DNS_ZONE_ID: string;
}

type ScheduledDependencies = {
  now(): Date;
  repository(env: Env): TicketRepository;
  dns(env: Env): DnsProvider;
  logger: CleanupLogger;
};

const consoleLogger: CleanupLogger = {
  info: (value) => console.info(value),
  error: (value) => console.error(value),
};

const defaultDependencies: ScheduledDependencies = {
  now: () => new Date(),
  repository: (env) => new D1TicketRepository(env.DB),
  dns: (env) => new CloudflareDnsProvider(env.CLOUDFLARE_DNS_TOKEN, env.CLOUDFLARE_DNS_ZONE_ID),
  logger: consoleLogger,
};

export function createScheduledHandler(dependencies: ScheduledDependencies) {
  return async (_controller: ScheduledController, env: Env) => {
    await cleanupExpiredEdgeTickets(
      dependencies.now(),
      dependencies.repository(env),
      dependencies.dns(env),
      dependencies.logger,
    );
  };
}

export default {
  scheduled: createScheduledHandler(defaultDependencies),
} satisfies ExportedHandler<Env>;
