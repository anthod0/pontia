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
  dns(env: Env): DnsProvider & { cleanupExpiredTxt(now: Date): Promise<number> };
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
    const dns = dependencies.dns(env);
    try {
      const deleted = await dns.cleanupExpiredTxt(dependencies.now());
      dependencies.logger.info({ event: "edge_acme_txt_cleanup", deleted });
    } catch {
      dependencies.logger.error({ event: "edge_acme_txt_cleanup_failed" });
    }
    await cleanupExpiredEdgeTickets(
      dependencies.now(),
      dependencies.repository(env),
      dns,
      dependencies.logger,
    );
  };
}

export default {
  scheduled: createScheduledHandler(defaultDependencies),
} satisfies ExportedHandler<Env>;
