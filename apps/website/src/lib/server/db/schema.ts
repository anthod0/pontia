import { sql } from "drizzle-orm";
import {
  check,
  index,
  integer,
  primaryKey,
  sqliteTable,
  text,
  uniqueIndex,
} from "drizzle-orm/sqlite-core";

const timestamp = sql`(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))`;

export const users = sqliteTable(
  "users",
  {
    id: text().notNull(),
    displayName: text("display_name"),
    avatarUrl: text("avatar_url"),
    createdAt: text("created_at").notNull().default(timestamp),
  },
  (table) => [primaryKey({ columns: [table.id] })],
);

export const edges = sqliteTable(
  "edges",
  {
    id: text().notNull(),
    name: text().notNull(),
    tunnelUrl: text("tunnel_url").notNull(),
    serviceCredentialHash: text("service_credential_hash").notNull(),
    createdAt: text("created_at").notNull().default(timestamp),
    updatedAt: text("updated_at").notNull().default(timestamp),
  },
  (table) => [primaryKey({ columns: [table.id] })],
);

export const devices = sqliteTable(
  "devices",
  {
    id: text().notNull(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    edgeId: text("edge_id")
      .notNull()
      .references(() => edges.id, { onDelete: "restrict" }),
    name: text(),
    createdAt: text("created_at").notNull().default(timestamp),
    updatedAt: text("updated_at").notNull().default(timestamp),
  },
  (table) => [
    primaryKey({ columns: [table.id] }),
    index("idx_devices_user_id").on(table.userId),
    index("idx_devices_edge_id").on(table.edgeId),
  ],
);

export const tunnelTickets = sqliteTable(
  "tunnel_tickets",
  {
    id: text().notNull(),
    secretHash: text("secret_hash").notNull(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    deviceId: text("device_id")
      .notNull()
      .references(() => devices.id, { onDelete: "cascade" }),
    edgeId: text("edge_id")
      .notNull()
      .references(() => edges.id, { onDelete: "cascade" }),
    expiresAt: text("expires_at").notNull(),
    consumedAt: text("consumed_at"),
    createdAt: text("created_at").notNull().default(timestamp),
  },
  (table) => [
    primaryKey({ columns: [table.id] }),
    index("idx_tunnel_tickets_expires_at").on(table.expiresAt),
  ],
);

export const accounts = sqliteTable(
  "accounts",
  {
    id: text().notNull(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    provider: text({ enum: ["google", "github"] }).notNull(),
    providerSubject: text("provider_subject").notNull(),
    email: text(),
    emailVerified: integer("email_verified", { mode: "boolean" })
      .notNull()
      .default(sql`0`),
    createdAt: text("created_at").notNull().default(timestamp),
    updatedAt: text("updated_at").notNull().default(timestamp),
  },
  (table) => [
    primaryKey({ columns: [table.id] }),
    check("accounts_provider_check", sql`${table.provider} IN ('google', 'github')`),
    uniqueIndex("idx_accounts_provider_subject").on(table.provider, table.providerSubject),
    uniqueIndex("idx_accounts_user_provider").on(table.userId, table.provider),
  ],
);

export const authSessions = sqliteTable(
  "auth_sessions",
  {
    id: text().notNull(),
    userId: text("user_id")
      .notNull()
      .references(() => users.id, { onDelete: "cascade" }),
    accountId: text("account_id").references(() => accounts.id, {
      onDelete: "set null",
    }),
    kind: text({ enum: ["browser", "cli"] })
      .notNull()
      .default("browser"),
    tokenHash: text("token_hash"),
    expiresAt: text("expires_at"),
    createdAt: text("created_at").notNull().default(timestamp),
  },
  (table) => [
    primaryKey({ columns: [table.id] }),
    index("idx_auth_sessions_user_id").on(table.userId),
  ],
);

export const deviceAuthorizations = sqliteTable(
  "device_authorizations",
  {
    id: text().notNull(),
    deviceCodeHash: text("device_code_hash").notNull(),
    userCode: text("user_code").notNull(),
    status: text({
      enum: ["pending", "approved", "denied", "consumed"],
    }).notNull(),
    userId: text("user_id").references(() => users.id, {
      onDelete: "cascade",
    }),
    expiresAt: text("expires_at").notNull(),
    lastPolledAt: text("last_polled_at"),
    createdAt: text("created_at").notNull().default(timestamp),
  },
  (table) => [
    primaryKey({ columns: [table.id] }),
    uniqueIndex("idx_device_authorizations_device_code").on(table.deviceCodeHash),
    uniqueIndex("idx_device_authorizations_user_code").on(table.userCode),
    index("idx_device_authorizations_expires_at").on(table.expiresAt),
    check(
      "device_authorizations_status_check",
      sql`${table.status} IN ('pending', 'approved', 'denied', 'consumed')`,
    ),
  ],
);

export const deviceRateLimits = sqliteTable(
  "device_rate_limits",
  {
    key: text().notNull(),
    windowStartedAt: text("window_started_at").notNull(),
    attemptCount: integer("attempt_count").notNull(),
  },
  (table) => [primaryKey({ columns: [table.key] })],
);
