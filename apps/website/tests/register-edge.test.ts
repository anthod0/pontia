import { describe, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import {
	buildEdgeInsertSql,
	createEdgeCredential,
	parseWranglerOutput,
	runRegistration,
	validateRegistrationInput,
	type Credential,
	type D1Client,
	type EdgeRecord,
	type RegistrationDependencies,
	type RegistrationPrompter
} from '../scripts/register-edge';

describe('registration input validation', () => {
	test('trims values and accepts a WSS URL with a path and port', () => {
		expect(
			validateRegistrationInput({
				environment: 'staging',
				edgeId: ' edge-eu-1 ',
				name: ' EU West ',
				tunnelUrl: ' wss://edge.example:8443/tunnel?region=eu '
			})
		).toEqual({
			environment: 'staging',
			edgeId: 'edge-eu-1',
			name: 'EU West',
			tunnelUrl: 'wss://edge.example:8443/tunnel?region=eu'
		});
	});

	test.each(['', '-edge', 'edge-', 'Edge', 'edge_name', 'a'.repeat(64)])(
		'rejects invalid edge ID %p',
		(edgeId) => {
			expect(() =>
				validateRegistrationInput({
					environment: 'production',
					edgeId,
					name: 'Edge',
					tunnelUrl: 'wss://edge.example/tunnel'
				})
			).toThrow('Edge ID');
		}
	);

	test('rejects a blank display name', () => {
		expect(() =>
			validateRegistrationInput({
				environment: 'production',
				edgeId: 'edge-1',
				name: '  ',
				tunnelUrl: 'wss://edge.example/tunnel'
			})
		).toThrow('Display name');
	});

	test.each([
		'https://edge.example/tunnel',
		'wss://',
		'wss://user:password@edge.example/tunnel',
		'wss://edge.example/tunnel#fragment'
	])('rejects invalid tunnel URL %p', (tunnelUrl) => {
		expect(() =>
			validateRegistrationInput({
				environment: 'production',
				edgeId: 'edge-1',
				name: 'Edge',
				tunnelUrl
			})
		).toThrow('Tunnel URL');
	});
});

test('credential uses 32 random bytes and hashes only the encoded secret', () => {
	const source = new Uint8Array(32).fill(7);
	const expectedSecret = Buffer.from(source).toString('base64url');
	const credential = createEdgeCredential('edge-1', (size) => {
		expect(size).toBe(32);
		return source;
	});

	expect(credential.value).toBe(`pec_v1_edge-1_${expectedSecret}`);
	expect(credential.hash).toBe(
		createHash('sha256').update(expectedSecret, 'utf8').digest('base64url')
	);
	expect(source).toEqual(new Uint8Array(32));
});

test.each([
	'not json',
	'[]',
	'[{}]',
	'[{"success":false}]',
	'[{"success":"true"}]'
])('Wrangler output must explicitly confirm success: %s', (stdout) => {
	expect(() => parseWranglerOutput(stdout)).toThrow();
});

test('insert SQL escapes values and contains only the credential hash', () => {
	const sql = buildEdgeInsertSql({
		id: 'edge-1',
		name: "O'Brien's Edge",
		tunnelUrl: "wss://edge.example/tunnel?label=operator's",
		serviceCredentialHash: 'safe-hash'
	});

	expect(sql).toContain("'O''Brien''s Edge'");
	expect(sql).toContain("'wss://edge.example/tunnel?label=operator''s'");
	expect(sql).toContain("'safe-hash'");
	expect(sql).not.toContain('pec_v1_');
	expect(sql).not.toMatch(/INSERT\s+OR\s+REPLACE|UPSERT|ON\s+CONFLICT/i);
});

class Answers implements RegistrationPrompter {
	constructor(private readonly answers: string[]) {}

	async question(): Promise<string> {
		const answer = this.answers.shift();
		if (answer === undefined) throw new Error('Unexpected prompt');
		return answer;
	}

	close(): void {}
}

function registrationHarness(options?: {
	answers?: string[];
	exists?: boolean;
	queryError?: Error;
	insertError?: Error;
	credential?: Credential;
}) {
	const prompter = new Answers(
		options?.answers ?? [
			'staging',
			'edge-1',
			'Edge One',
			'wss://edge.example/tunnel',
			'yes'
		]
	);
	const output: string[] = [];
	const errors: string[] = [];
	const inserted: EdgeRecord[] = [];
	let credentialCalls = 0;
	const d1: D1Client = {
		async edgeExists() {
			if (options?.queryError) throw options.queryError;
			return options?.exists ?? false;
		},
		async insertEdge(_environment, edge) {
			inserted.push(edge);
			if (options?.insertError) throw options.insertError;
		}
	};
	const credential = options?.credential ?? {
		value: 'pec_v1_edge-1_test-secret',
		hash: 'test-hash'
	};
	const dependencies: RegistrationDependencies = {
		d1,
		prompter,
		write(message) {
			output.push(message);
		},
		writeError(message) {
			errors.push(message);
		},
		createCredential() {
			credentialCalls += 1;
			return credential;
		}
	};
	return {
		dependencies,
		prompter,
		output,
		errors,
		inserted,
		credential,
		get credentialCalls() {
			return credentialCalls;
		}
	};
}

test('successful registration reveals the credential only after insertion', async () => {
	const harness = registrationHarness();
	let outputAtInsert = '';
	const d1 = harness.dependencies.d1;
	harness.dependencies.d1 = {
		edgeExists: d1.edgeExists,
		async insertEdge(environment, edge) {
			outputAtInsert = harness.output.join('\n');
			await d1.insertEdge(environment, edge);
		}
	};

	expect(await runRegistration(harness.dependencies)).toBe(0);
	expect(outputAtInsert).not.toContain(harness.credential.value);
	expect(harness.output.join('\n')).toContain(harness.credential.value);
	expect(harness.inserted).toEqual([
		{
			id: 'edge-1',
			name: 'Edge One',
			tunnelUrl: 'wss://edge.example/tunnel',
			serviceCredentialHash: 'test-hash'
		}
	]);
});

test('cancellation performs no query and generates no credential', async () => {
	const harness = registrationHarness({
		answers: [
			'production',
			'edge-1',
			'Edge One',
			'wss://edge.example/tunnel',
			'no'
		]
	});
	let queried = false;
	harness.dependencies.d1 = {
		async edgeExists() {
			queried = true;
			return false;
		},
		async insertEdge() {
			throw new Error('should not insert');
		}
	};

	expect(await runRegistration(harness.dependencies)).toBe(0);
	expect(queried).toBe(false);
	expect(harness.credentialCalls).toBe(0);
	expect(harness.output.join('\n')).toContain('cancelled');
});

test('existing edge ID fails before credential generation', async () => {
	const harness = registrationHarness({ exists: true });

	expect(await runRegistration(harness.dependencies)).toBe(1);
	expect(harness.credentialCalls).toBe(0);
	expect(harness.inserted).toHaveLength(0);
	expect(harness.errors.join('\n')).toContain('already exists');
});

test.each([
	['query', { queryError: new Error('Wrangler unavailable') }],
	['insert', { insertError: new Error('primary key conflict') }]
] as const)(
	'%s failure never reveals the credential',
	async (_name, options) => {
		const harness = registrationHarness(options);

		expect(await runRegistration(harness.dependencies)).toBe(1);
		expect(harness.output.join('\n')).not.toContain(harness.credential.value);
		expect(harness.errors.join('\n')).not.toContain(harness.credential.value);
	}
);
