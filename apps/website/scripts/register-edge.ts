import { createHash, randomBytes } from 'node:crypto';
import { chmod, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline/promises';

export type RegistrationEnvironment = 'production' | 'staging';

export interface RegistrationInput {
	environment: RegistrationEnvironment;
	edgeId: string;
	name: string;
	tunnelUrl: string;
}

export interface EdgeRecord {
	id: string;
	name: string;
	tunnelUrl: string;
	serviceCredentialHash: string;
}

export interface Credential {
	value: string;
	hash: string;
}

export interface D1Client {
	edgeExists(
		environment: RegistrationEnvironment,
		edgeId: string
	): Promise<boolean>;
	insertEdge(
		environment: RegistrationEnvironment,
		edge: EdgeRecord
	): Promise<void>;
}

export interface RegistrationPrompter {
	question(message: string): Promise<string>;
	close(): void;
}

export interface RegistrationDependencies {
	d1: D1Client;
	prompter: RegistrationPrompter;
	write(message: string): void;
	writeError(message: string): void;
	createCredential(edgeId: string): Credential;
}

const EDGE_ID_PATTERN = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const CONTROL_CHARACTER_PATTERN = /[\u0000-\u001f\u007f-\u009f]/;
const WEBSITE_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));

export function validateRegistrationInput(
	input: RegistrationInput
): RegistrationInput {
	const edgeId = input.edgeId.trim();
	const name = input.name.trim();
	const tunnelUrl = input.tunnelUrl.trim();

	if (!EDGE_ID_PATTERN.test(edgeId)) {
		throw new Error(
			'Edge ID must be 1-63 lowercase letters, numbers, or hyphens, and cannot start or end with a hyphen.'
		);
	}
	if (!name) throw new Error('Display name is required.');
	if (CONTROL_CHARACTER_PATTERN.test(name)) {
		throw new Error('Display name cannot contain control characters.');
	}

	let parsedTunnelUrl: URL;
	try {
		parsedTunnelUrl = new URL(tunnelUrl);
	} catch {
		throw new Error('Tunnel URL must be a valid WSS URL.');
	}
	if (parsedTunnelUrl.protocol !== 'wss:' || !parsedTunnelUrl.hostname) {
		throw new Error('Tunnel URL must use wss:// and include a hostname.');
	}
	if (parsedTunnelUrl.username || parsedTunnelUrl.password) {
		throw new Error('Tunnel URL cannot include credentials.');
	}
	if (parsedTunnelUrl.hash)
		throw new Error('Tunnel URL cannot include a fragment.');

	return {
		environment: input.environment,
		edgeId,
		name,
		tunnelUrl: parsedTunnelUrl.toString()
	};
}

export function createEdgeCredential(
	edgeId: string,
	bytes: (size: number) => Uint8Array = randomBytes
): Credential {
	const secretBytes = bytes(32);
	if (secretBytes.length !== 32)
		throw new Error('Credential generator must return 32 bytes.');

	try {
		const secret = Buffer.from(secretBytes).toString('base64url');
		return {
			value: `pec_v1_${edgeId}_${secret}`,
			hash: createHash('sha256').update(secret, 'utf8').digest('base64url')
		};
	} finally {
		secretBytes.fill(0);
	}
}

function sqlString(value: string): string {
	if (value.includes('\0'))
		throw new Error('SQL values cannot contain NUL bytes.');
	return `'${value.replaceAll("'", "''")}'`;
}

export function buildEdgeLookupSql(edgeId: string): string {
	return `SELECT id FROM edges WHERE id = ${sqlString(edgeId)} LIMIT 1;\n`;
}

export function buildEdgeInsertSql(edge: EdgeRecord): string {
	return [
		'INSERT INTO edges (id, name, tunnel_url, service_credential_hash)',
		'VALUES (',
		`  ${sqlString(edge.id)},`,
		`  ${sqlString(edge.name)},`,
		`  ${sqlString(edge.tunnelUrl)},`,
		`  ${sqlString(edge.serviceCredentialHash)}`,
		');',
		''
	].join('\n');
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
		throw new Error('Wrangler returned invalid JSON.');
	}
	if (!Array.isArray(output) || output.length !== 1) {
		throw new Error('Wrangler returned an unexpected result.');
	}
	const result = output[0];
	if (!result || typeof result !== 'object' || !('success' in result)) {
		throw new Error('Wrangler returned an unexpected result.');
	}
	if (result.success !== true) {
		throw new Error('Wrangler reported an unsuccessful operation.');
	}
	return result as WranglerResult;
}

export class WranglerD1Client implements D1Client {
	async edgeExists(
		environment: RegistrationEnvironment,
		edgeId: string
	): Promise<boolean> {
		const result = await this.execute(environment, buildEdgeLookupSql(edgeId));
		if (!Array.isArray(result.results)) {
			throw new Error('Wrangler returned an invalid query result.');
		}
		return result.results.length > 0;
	}

	async insertEdge(
		environment: RegistrationEnvironment,
		edge: EdgeRecord
	): Promise<void> {
		await this.execute(environment, buildEdgeInsertSql(edge));
	}

	private async execute(
		environment: RegistrationEnvironment,
		sql: string
	): Promise<WranglerResult> {
		const temporaryDirectory = await mkdtemp(
			join(tmpdir(), 'pontia-register-edge-')
		);
		const sqlPath = join(temporaryDirectory, 'statement.sql');
		try {
			await chmod(temporaryDirectory, 0o700);
			await writeFile(sqlPath, sql, { encoding: 'utf8', mode: 0o600 });
			const config =
				environment === 'production'
					? join(WEBSITE_ROOT, 'wrangler.toml')
					: join(WEBSITE_ROOT, 'wrangler.preview-migrations.toml');
			const executable = join(
				WEBSITE_ROOT,
				'node_modules',
				'.bin',
				process.platform === 'win32' ? 'wrangler.cmd' : 'wrangler'
			);
			let processResult;
			try {
				processResult = Bun.spawn(
					[
						executable,
						'd1',
						'execute',
						'DB',
						'--remote',
						'--config',
						config,
						'--file',
						sqlPath,
						'--json',
						'--yes'
					],
					{ cwd: WEBSITE_ROOT, stdout: 'pipe', stderr: 'pipe' }
				);
			} catch (error) {
				throw new Error(
					`Wrangler could not be started: ${errorMessage(error)}`
				);
			}

			const [exitCode, stdout, stderr] = await Promise.all([
				processResult.exited,
				new Response(processResult.stdout).text(),
				new Response(processResult.stderr).text()
			]);
			if (exitCode !== 0) {
				throw new Error(
					`Wrangler failed: ${stderr.trim() || `exit code ${exitCode}`}`
				);
			}

			return parseWranglerOutput(stdout);
		} finally {
			const resolvedDirectory = resolve(temporaryDirectory);
			if (!resolvedDirectory.startsWith(resolve(tmpdir()) + sep)) {
				throw new Error('Refusing to clean an invalid temporary directory.');
			}
			await rm(temporaryDirectory, { recursive: true, force: true });
		}
	}
}

export async function runRegistration(
	dependencies: RegistrationDependencies
): Promise<number> {
	const { d1, prompter, write, writeError, createCredential } = dependencies;
	try {
		const environmentAnswer = (
			await prompter.question(
				'Target environment (production/staging) [production]: '
			)
		)
			.trim()
			.toLowerCase();
		if (
			environmentAnswer &&
			environmentAnswer !== 'production' &&
			environmentAnswer !== 'staging'
		) {
			throw new Error('Environment must be production or staging.');
		}

		const input = validateRegistrationInput({
			environment: environmentAnswer === 'staging' ? 'staging' : 'production',
			edgeId: await prompter.question('Edge ID: '),
			name: await prompter.question('Display name: '),
			tunnelUrl: await prompter.question('WSS tunnel URL: ')
		});

		write('\nRegistration summary:');
		write(`  Environment: ${input.environment}`);
		write(`  Edge ID: ${input.edgeId}`);
		write(`  Display name: ${input.name}`);
		write(`  Tunnel URL: ${input.tunnelUrl}`);
		const confirmation = (
			await prompter.question('\nType "yes" to create this edge: ')
		)
			.trim()
			.toLowerCase();
		if (confirmation !== 'yes') {
			write('Registration cancelled. No database changes were made.');
			return 0;
		}

		write('Checking whether the edge ID is available...');
		if (await d1.edgeExists(input.environment, input.edgeId)) {
			throw new Error(
				`Edge ID "${input.edgeId}" already exists; no record was changed.`
			);
		}

		const credential = createCredential(input.edgeId);
		await d1.insertEdge(input.environment, {
			id: input.edgeId,
			name: input.name,
			tunnelUrl: input.tunnelUrl,
			serviceCredentialHash: credential.hash
		});

		write('\nEdge registered successfully. Save this credential now:');
		write(credential.value);
		write(
			'\nThe website stores only its hash, and this script cannot display or recover it again.'
		);
		write('Configure the credential on the matching edge manually.');
		write(
			'The edge is registered but is not online until deployment and configuration are complete.'
		);
		return 0;
	} catch (error) {
		writeError(`Registration failed: ${errorMessage(error)}`);
		return 1;
	} finally {
		prompter.close();
	}
}

function errorMessage(error: unknown): string {
	return error instanceof Error ? error.message : String(error);
}

if (import.meta.main) {
	if (!process.stdin.isTTY || !process.stdout.isTTY) {
		console.error(
			'Registration failed: this command requires an interactive terminal.'
		);
		process.exitCode = 1;
	} else {
		const prompter = createInterface({
			input: process.stdin,
			output: process.stdout
		});
		process.exitCode = await runRegistration({
			d1: new WranglerD1Client(),
			prompter,
			write: console.log,
			writeError: console.error,
			createCredential: createEdgeCredential
		});
	}
}
