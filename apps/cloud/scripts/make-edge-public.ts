import { chmod, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline/promises";
import { validate as validateUuid, version as uuidVersion } from "uuid";

export type TargetEnvironment = "production" | "staging";

export interface CommandInput {
  edgeId: string;
  environment: TargetEnvironment;
}

export interface EdgeRecord {
  id: string;
  name: string;
  tunnelUrl: string;
  accessScope: "private" | "public";
}

export interface D1Client {
  findEdge(environment: TargetEnvironment, edgeId: string): Promise<EdgeRecord | null>;
  makeEdgePublic(environment: TargetEnvironment, edgeId: string): Promise<void>;
}

export interface Prompter {
  question(message: string): Promise<string>;
  close(): void;
}

export interface CommandDependencies {
  d1: D1Client;
  prompter: Prompter;
  write(message: string): void;
  writeError(message: string): void;
}

const CLOUD_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

export function parseCommandInput(args: readonly string[]): CommandInput {
  if (args.length !== 2) {
    throw new Error("Usage: bun run scripts/make-edge-public.ts <edge-id> <production|staging>");
  }
  const [edgeId, environment] = args;
  if (!validateUuid(edgeId) || uuidVersion(edgeId) !== 7 || edgeId !== edgeId.toLowerCase()) {
    throw new Error("Edge ID must be a canonical lowercase UUIDv7.");
  }
  if (environment !== "production" && environment !== "staging") {
    throw new Error("Environment must be production or staging.");
  }
  return { edgeId, environment };
}

function sqlString(value: string): string {
  if (value.includes("\0")) throw new Error("SQL values cannot contain NUL bytes.");
  return `'${value.replaceAll("'", "''")}'`;
}

export function buildEdgeLookupSql(edgeId: string): string {
  return [
    "SELECT id, name, tunnel_url, access_scope",
    "FROM edges",
    `WHERE id = ${sqlString(edgeId)}`,
    "LIMIT 1;",
    "",
  ].join("\n");
}

export function buildMakePublicSql(edgeId: string): string {
  return [
    "UPDATE edges",
    "SET access_scope = 'public',",
    "    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
    `WHERE id = ${sqlString(edgeId)} AND access_scope = 'private'`,
    "RETURNING id, access_scope;",
    "",
  ].join("\n");
}

interface WranglerResult {
  success: true;
  results?: unknown[];
}

export function parseWranglerOutput(stdout: string): WranglerResult {
  let output: unknown;
  try {
    output = JSON.parse(stdout);
  } catch {
    throw new Error("Wrangler returned invalid JSON.");
  }
  if (!Array.isArray(output) || output.length !== 1) {
    throw new Error("Wrangler returned an unexpected result.");
  }
  const result = output[0];
  if (!result || typeof result !== "object" || !("success" in result) || result.success !== true) {
    throw new Error("Wrangler reported an unsuccessful operation.");
  }
  return result as WranglerResult;
}

function parseEdgeRow(value: unknown): EdgeRecord {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Wrangler returned an invalid edge record.");
  }
  const row = value as Record<string, unknown>;
  if (
    typeof row.id !== "string" ||
    typeof row.name !== "string" ||
    typeof row.tunnel_url !== "string" ||
    (row.access_scope !== "private" && row.access_scope !== "public")
  ) {
    throw new Error("Wrangler returned an invalid edge record.");
  }
  return {
    id: row.id,
    name: row.name,
    tunnelUrl: row.tunnel_url,
    accessScope: row.access_scope,
  };
}

export class WranglerD1Client implements D1Client {
  async findEdge(environment: TargetEnvironment, edgeId: string): Promise<EdgeRecord | null> {
    const result = await this.execute(environment, buildEdgeLookupSql(edgeId));
    if (!Array.isArray(result.results)) {
      throw new Error("Wrangler returned an invalid query result.");
    }
    if (result.results.length === 0) return null;
    if (result.results.length !== 1) {
      throw new Error("Wrangler returned multiple records for one edge ID.");
    }
    return parseEdgeRow(result.results[0]);
  }

  async makeEdgePublic(environment: TargetEnvironment, edgeId: string): Promise<void> {
    const result = await this.execute(environment, buildMakePublicSql(edgeId));
    if (!Array.isArray(result.results) || result.results.length !== 1) {
      throw new Error("The edge changed before it could be made public; no update was confirmed.");
    }
    const row = result.results[0];
    if (
      !row ||
      typeof row !== "object" ||
      Array.isArray(row) ||
      (row as Record<string, unknown>).id !== edgeId ||
      (row as Record<string, unknown>).access_scope !== "public"
    ) {
      throw new Error("Wrangler returned an invalid update result.");
    }
  }

  private async execute(environment: TargetEnvironment, sql: string): Promise<WranglerResult> {
    const temporaryDirectory = await mkdtemp(join(tmpdir(), "pontia-make-edge-public-"));
    const sqlPath = join(temporaryDirectory, "statement.sql");
    try {
      await chmod(temporaryDirectory, 0o700);
      await writeFile(sqlPath, sql, { encoding: "utf8", mode: 0o600 });
      const config =
        environment === "production"
          ? join(CLOUD_ROOT, "wrangler.toml")
          : join(CLOUD_ROOT, "wrangler.preview-migrations.toml");
      const executable = join(
        CLOUD_ROOT,
        "node_modules",
        ".bin",
        process.platform === "win32" ? "wrangler.cmd" : "wrangler",
      );
      let child;
      try {
        child = Bun.spawn(
          [
            executable,
            "d1",
            "execute",
            "DB",
            "--remote",
            "--config",
            config,
            "--file",
            sqlPath,
            "--json",
            "--yes",
          ],
          { cwd: CLOUD_ROOT, stdout: "pipe", stderr: "pipe" },
        );
      } catch (error) {
        throw new Error(`Wrangler could not be started: ${errorMessage(error)}`);
      }
      const [exitCode, stdout, stderr] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      if (exitCode !== 0) {
        throw new Error(`Wrangler failed: ${stderr.trim() || `exit code ${exitCode}`}`);
      }
      return parseWranglerOutput(stdout);
    } finally {
      const resolvedDirectory = resolve(temporaryDirectory);
      if (!resolvedDirectory.startsWith(resolve(tmpdir()) + sep)) {
        throw new Error("Refusing to clean an invalid temporary directory.");
      }
      await rm(temporaryDirectory, { recursive: true, force: true });
    }
  }
}

export async function runCommand(
  input: CommandInput,
  dependencies: CommandDependencies,
): Promise<number> {
  const { d1, prompter, write, writeError } = dependencies;
  try {
    const edge = await d1.findEdge(input.environment, input.edgeId);
    if (!edge) throw new Error(`Edge ${input.edgeId} was not found in ${input.environment}.`);

    write("Edge:");
    write(`  Environment: ${input.environment}`);
    write(`  ID: ${edge.id}`);
    write(`  Name: ${edge.name}`);
    write(`  Tunnel URL: ${edge.tunnelUrl}`);
    write(`  Current scope: ${edge.accessScope}`);

    if (edge.accessScope === "public") {
      write("\nThis edge is already public. No database changes were made.");
      return 0;
    }

    const confirmation = (await prompter.question('\nType "yes" to make this edge public: '))
      .trim()
      .toLowerCase();
    if (confirmation !== "yes") {
      write("Change cancelled. No database changes were made.");
      return 0;
    }

    await d1.makeEdgePublic(input.environment, input.edgeId);
    write("\nEdge is now public.");
    return 0;
  } catch (error) {
    writeError(`Failed to make edge public: ${errorMessage(error)}`);
    return 1;
  } finally {
    prompter.close();
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function main(): Promise<number> {
  let input: CommandInput;
  try {
    input = parseCommandInput(process.argv.slice(2));
  } catch (error) {
    console.error(errorMessage(error));
    return 1;
  }

  if (!process.stdin.isTTY || !process.stdout.isTTY) {
    console.error("This command requires an interactive terminal.");
    return 1;
  }

  const prompter = createInterface({ input: process.stdin, output: process.stdout });
  return runCommand(input, {
    d1: new WranglerD1Client(),
    prompter,
    write: console.log,
    writeError: console.error,
  });
}

if (import.meta.main) process.exitCode = await main();
