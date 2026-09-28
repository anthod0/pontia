import { describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import {
  buildEdgeInsertSql,
  parseEdgeCredential,
  parseWranglerOutput,
  runRegistration,
  validateRegistrationInput,
  type D1Client,
  type EdgeRecord,
  type RegistrationDependencies,
  type RegistrationPrompter,
} from "../scripts/register-edge";

const edgeId = "01a0e686-24d4-75e8-866d-5710ffe3b2b5";
const secret = Buffer.from(new Uint8Array(32).fill(7)).toString("base64url");
const pec = `pec_v1_${edgeId}_${secret}`;
const credentialHash = createHash("sha256").update(secret, "utf8").digest("base64url");

describe("registration input validation", () => {
  test("trims values and accepts a WSS URL with a path and port", () => {
    expect(
      validateRegistrationInput({
        environment: "staging",
        name: " EU West ",
        tunnelUrl: " wss://edge.example:8443/tunnel?region=eu ",
      }),
    ).toEqual({
      environment: "staging",
      name: "EU West",
      tunnelUrl: "wss://edge.example:8443/tunnel?region=eu",
    });
  });

  test("rejects a blank display name", () => {
    expect(() =>
      validateRegistrationInput({
        environment: "production",
        name: "  ",
        tunnelUrl: "wss://edge.example/tunnel",
      }),
    ).toThrow("Display name");
  });

  test.each([
    "https://edge.example/tunnel",
    "wss://",
    "wss://user:password@edge.example/tunnel",
    "wss://edge.example/tunnel#fragment",
  ])("rejects invalid tunnel URL %p", (tunnelUrl) => {
    expect(() =>
      validateRegistrationInput({
        environment: "production",
        name: "Edge",
        tunnelUrl,
      }),
    ).toThrow("Tunnel URL");
  });
});

test("credential parser extracts UUID v7 and hashes only the encoded secret", () => {
  expect(parseEdgeCredential(pec)).toEqual({
    edgeId,
    hash: credentialHash,
  });
});

test.each([
  ` ${pec}`,
  `${pec} `,
  `${pec}\n`,
  `${pec}_extra`,
  pec.replace("pec_v1_", "pec_v2_"),
  pec.replace(edgeId, "not-a-uuid"),
  pec.replace(edgeId, "01a0e686-24d4-45e8-866d-5710ffe3b2b5"),
  pec.replace(secret, `${secret.slice(0, 42)}=`),
  pec.replace(secret, `${secret.slice(0, 42)}+`),
])("credential parser rejects non-canonical input", (value) => {
  expect(() => parseEdgeCredential(value)).toThrow("Edge credential is invalid.");
});

test.each(["not json", "[]", "[{}]", '[{"success":false}]', '[{"success":"true"}]'])(
  "Wrangler output must explicitly confirm success: %s",
  (stdout) => {
    expect(() => parseWranglerOutput(stdout)).toThrow();
  },
);

test("insert SQL escapes values and contains only the credential hash", () => {
  const sql = buildEdgeInsertSql({
    id: edgeId,
    name: "O'Brien's Edge",
    tunnelUrl: "wss://edge.example/tunnel?label=operator's",
    serviceCredentialHash: credentialHash,
  });

  expect(sql).toContain("'O''Brien''s Edge'");
  expect(sql).toContain("'wss://edge.example/tunnel?label=operator''s'");
  expect(sql).toContain(`'${credentialHash}'`);
  expect(sql).not.toContain(secret);
  expect(sql).not.toMatch(/INSERT\s+OR\s+REPLACE|UPSERT|ON\s+CONFLICT/i);
});

class Answers implements RegistrationPrompter {
  constructor(private readonly answers: string[]) {}

  async question(): Promise<string> {
    const answer = this.answers.shift();
    if (answer === undefined) throw new Error("Unexpected prompt");
    return answer;
  }

  close(): void {}
}

function registrationHarness(options?: {
  answers?: string[];
  exists?: boolean;
  queryError?: Error;
  insertError?: Error;
}) {
  const prompter = new Answers(
    options?.answers ?? [pec, "staging", "Edge One", "wss://edge.example/tunnel", "yes"],
  );
  const output: string[] = [];
  const errors: string[] = [];
  const inserted: EdgeRecord[] = [];
  const d1: D1Client = {
    async edgeExists() {
      if (options?.queryError) throw options.queryError;
      return options?.exists ?? false;
    },
    async insertEdge(_environment, edge) {
      inserted.push(edge);
      if (options?.insertError) throw options.insertError;
    },
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
  };
  return { dependencies, output, errors, inserted };
}

test("successful registration uses the parsed ID and never repeats the credential", async () => {
  const harness = registrationHarness();

  expect(await runRegistration(harness.dependencies)).toBe(0);
  expect(harness.output.join("\n")).not.toContain(secret);
  expect(harness.inserted).toEqual([
    {
      id: edgeId,
      name: "Edge One",
      tunnelUrl: "wss://edge.example/tunnel",
      serviceCredentialHash: credentialHash,
    },
  ]);
});

test("cancellation performs no query or insert", async () => {
  const harness = registrationHarness({
    answers: [pec, "production", "Edge One", "wss://edge.example/tunnel", "no"],
  });
  let queried = false;
  harness.dependencies.d1 = {
    async edgeExists() {
      queried = true;
      return false;
    },
    async insertEdge() {
      throw new Error("should not insert");
    },
  };

  expect(await runRegistration(harness.dependencies)).toBe(0);
  expect(queried).toBe(false);
  expect(harness.output.join("\n")).toContain("cancelled");
});

test("an existing parsed edge ID is rejected without insertion", async () => {
  const harness = registrationHarness({ exists: true });

  expect(await runRegistration(harness.dependencies)).toBe(1);
  expect(harness.inserted).toHaveLength(0);
  expect(harness.errors.join("\n")).toContain("already exists");
  expect(harness.errors.join("\n")).not.toContain(secret);
});

test("an invalid credential fails before querying D1 without echoing input", async () => {
  const invalidCredential = `${pec} `;
  const harness = registrationHarness({ answers: [invalidCredential] });
  let queried = false;
  harness.dependencies.d1 = {
    async edgeExists() {
      queried = true;
      return false;
    },
    async insertEdge() {
      throw new Error("should not insert");
    },
  };

  expect(await runRegistration(harness.dependencies)).toBe(1);
  expect(queried).toBe(false);
  expect(harness.errors.join("\n")).toBe("Registration failed: Edge credential is invalid.");
  expect(harness.errors.join("\n")).not.toContain(invalidCredential);
});

test.each([
  ["query", { queryError: new Error("Wrangler unavailable") }],
  ["insert", { insertError: new Error("primary key conflict") }],
] as const)("%s failure never reveals the credential", async (_name, options) => {
  const harness = registrationHarness(options);

  expect(await runRegistration(harness.dependencies)).toBe(1);
  expect(harness.output.join("\n")).not.toContain(secret);
  expect(harness.errors.join("\n")).not.toContain(secret);
});
