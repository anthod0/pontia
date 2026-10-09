import { afterAll, expect, test } from "bun:test";
import { generateKeyPairSync, verify } from "node:crypto";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve, sep } from "node:path";

const repository = new URL("../../..", import.meta.url).pathname;
const roots: string[] = [];
const keys = generateKeyPairSync("ed25519", {
  publicKeyEncoding: { type: "spki", format: "pem" },
  privateKeyEncoding: { type: "pkcs8", format: "pem" },
});
const linuxArchitecture: "x86_64" | "arm64" = process.arch === "arm64" ? "arm64" : "x86_64";
const controlPlaneTargets = [
  "x86_64-unknown-linux-gnu",
  "aarch64-unknown-linux-gnu",
  "x86_64-apple-darwin",
  "aarch64-apple-darwin",
];

afterAll(async () => {
  for (const root of roots) {
    if (!root || !resolve(root).startsWith(resolve(tmpdir()) + sep)) {
      throw new Error("Invalid test root");
    }
    await rm(root, { recursive: true, force: true });
  }
});

async function fixture(
  script: string,
  platform: { system: "Linux" | "Darwin"; architecture: "x86_64" | "arm64" } = {
    system: "Linux",
    architecture: linuxArchitecture,
  },
) {
  const root = await mkdtemp(join(tmpdir(), "pontia-installer-"));
  roots.push(root);
  const releases = join(root, "releases");
  const mocks = join(root, "bin");
  const payloads = join(root, "payloads");
  await mkdir(releases);
  await mkdir(mocks);
  await mkdir(payloads);
  for (const binary of ["pontia", "pontiad", "pontia-edge"]) {
    await writeFile(join(payloads, binary), `verified ${binary} release binary\n`, { mode: 0o755 });
    const targets =
      binary === "pontia-edge" ? controlPlaneTargets.slice(0, 2) : controlPlaneTargets;
    for (const releaseTarget of targets) {
      const archive = join(releases, `${binary}-${releaseTarget}.tar.gz`);
      expect(Bun.spawnSync(["tar", "-czf", archive, "-C", payloads, binary]).exitCode).toBe(0);
    }
  }
  await writeFile(join(releases, "pontia-dashboard.tar.gz"), "dashboard fixture");
  const signingEnvironment = {
    ...process.env,
    RELEASE_PUBLIC_KEY: keys.publicKey,
    RELEASE_SIGNING_KEY: keys.privateKey,
  };
  for (const args of [
    ["prepare-release", releases, "--version", "v1.2.3"],
    ["prepare-installers", root],
  ]) {
    const prepared = Bun.spawnSync(["python3", ".github/scripts/publish-release.py", ...args], {
      cwd: repository,
      env: signingEnvironment,
    });
    expect(prepared.exitCode).toBe(0);
  }
  const envelope = JSON.parse(await readFile(join(releases, "manifest.json"), "utf8"));
  const signature = Buffer.from(envelope.signature, "base64");
  expect(signature.length).toBe(64);
  expect(verify(null, Buffer.from(envelope.manifest, "base64"), keys.publicKey, signature)).toBe(
    true,
  );
  const curl = join(mocks, "curl");
  await writeFile(
    curl,
    `#!/bin/sh
set -eu
url="$2"
out="$4"
printf '%s\\n' "$url" >> "$TEST_REQUESTS"
case "$url" in
  */channels/stable.json) cp "$TEST_RELEASES/manifest.json" "$out" ;;
  *) cp "$TEST_RELEASES/\${url##*/}" "$out" ;;
esac
`,
  );
  await chmod(curl, 0o755);
  const uname = join(mocks, "uname");
  await writeFile(
    uname,
    `#!/bin/sh
case "$1" in
  -s) printf '%s\\n' '${platform.system}' ;;
  -m) printf '%s\\n' '${platform.architecture}' ;;
  *) exit 1 ;;
esac
`,
  );
  await chmod(uname, 0o755);
  const shasum = join(mocks, "shasum");
  await writeFile(
    shasum,
    `#!/bin/sh
[ "$1" = -a ] && [ "$2" = 256 ]
sha256sum "$3"
`,
  );
  await chmod(shasum, 0o755);
  const releaseTarget = `${platform.architecture === "arm64" ? "aarch64" : "x86_64"}-${platform.system === "Darwin" ? "apple-darwin" : "unknown-linux-gnu"}`;
  return {
    root,
    releases,
    target: releaseTarget,
    run(version: string) {
      return Bun.spawnSync(["sh", join(root, script)], {
        env: {
          ...process.env,
          PATH: `${mocks}:${process.env.PATH}`,
          PONTIA_INSTALL_DIR: join(root, "install"),
          PONTIA_EDGE_INSTALL_DIR: join(root, "install"),
          PONTIA_VERSION: version,
          PONTIA_EDGE_VERSION: version,
          TEST_RELEASES: releases,
          TEST_REQUESTS: join(root, "requests"),
        },
      });
    },
  };
}

test("publisher rejects mismatched Ed25519 signing keys", async () => {
  const context = await fixture("install.sh");
  const wrongKeys = generateKeyPairSync("ed25519", {
    privateKeyEncoding: { type: "pkcs8", format: "pem" },
  });
  const manifest = join(context.releases, "manifest.json");
  await rm(manifest);
  const result = Bun.spawnSync(
    [
      "python3",
      ".github/scripts/publish-release.py",
      "prepare-release",
      context.releases,
      "--version",
      "v1.2.3",
    ],
    {
      cwd: repository,
      env: {
        ...process.env,
        RELEASE_PUBLIC_KEY: keys.publicKey,
        RELEASE_SIGNING_KEY: wrongKeys.privateKey,
      },
    },
  );
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr.toString()).toContain(
    "RELEASE_SIGNING_KEY does not match RELEASE_PUBLIC_KEY",
  );
  expect(await Bun.file(manifest).exists()).toBe(false);
});

test("installer preparation rejects non-Ed25519 public keys", async () => {
  const root = await mkdtemp(join(tmpdir(), "pontia-installer-key-"));
  roots.push(root);
  const wrongKeys = generateKeyPairSync("ec", {
    namedCurve: "prime256v1",
    publicKeyEncoding: { type: "spki", format: "pem" },
  });
  const result = Bun.spawnSync(
    ["python3", ".github/scripts/publish-release.py", "prepare-installers", root],
    {
      cwd: repository,
      env: { ...process.env, RELEASE_PUBLIC_KEY: wrongKeys.publicKey },
    },
  );
  expect(result.exitCode).not.toBe(0);
  expect(result.stderr.toString()).toContain("RELEASE_PUBLIC_KEY must be an Ed25519 public key");
  expect(await Bun.file(join(root, "install.sh")).exists()).toBe(false);
});

for (const architecture of ["x86_64", "arm64"] as const) {
  test(`install.sh installs macOS ${architecture} releases`, async () => {
    const context = await fixture("install.sh", { system: "Darwin", architecture });
    const result = context.run("latest");

    expect(result.stderr.toString()).toBe("");
    expect(result.exitCode).toBe(0);
    for (const binary of ["pontia", "pontiad"]) {
      expect(await readFile(join(context.root, "install", binary), "utf8")).toBe(
        `verified ${binary} release binary\n`,
      );
    }
    expect(await readFile(join(context.root, "requests"), "utf8")).toContain(
      `pontia-${context.target}.tar.gz`,
    );
  });
}

for (const script of ["install.sh", "install-edge.sh"]) {
  const binaries = script === "install.sh" ? ["pontia", "pontiad"] : ["pontia-edge"];
  for (const version of ["latest", "v1.2.3"]) {
    test(`${script} verifies and installs ${version} from immutable release paths`, async () => {
      const context = await fixture(script);
      const result = context.run(version);
      expect(result.stderr.toString()).toBe("");
      expect(result.exitCode).toBe(0);
      for (const binary of binaries) {
        expect(await readFile(join(context.root, "install", binary), "utf8")).toBe(
          `verified ${binary} release binary\n`,
        );
      }
      const manifestPath =
        version === "latest" ? "channels/stable.json" : "releases/v1.2.3/manifest.json";
      expect(await readFile(join(context.root, "requests"), "utf8")).toBe(
        [
          `https://get.pontia.dev/${manifestPath}`,
          ...binaries.map(
            (binary) => `https://get.pontia.dev/releases/v1.2.3/${binary}-${context.target}.tar.gz`,
          ),
          "",
        ].join("\n"),
      );
    });
  }

  test(`${script} rejects a tampered manifest before downloading binaries`, async () => {
    const context = await fixture(script);
    const path = join(context.releases, "manifest.json");
    const envelope = JSON.parse(await readFile(path, "utf8"));
    const manifest = JSON.parse(Buffer.from(envelope.manifest, "base64").toString());
    manifest.version = "v9.9.9";
    envelope.manifest = Buffer.from(JSON.stringify(manifest)).toString("base64");
    await writeFile(path, JSON.stringify(envelope));
    expect(context.run("latest").exitCode).not.toBe(0);
    expect(await readFile(join(context.root, "requests"), "utf8")).toBe(
      "https://get.pontia.dev/channels/stable.json\n",
    );
    expect(await Bun.file(join(context.root, "install", binaries[0])).exists()).toBe(false);
  });

  test(`${script} rejects corrupted archives without replacing installed binaries`, async () => {
    const context = await fixture(script);
    await mkdir(join(context.root, "install"));
    for (const binary of binaries) {
      await writeFile(join(context.root, "install", binary), "original binary");
    }
    await writeFile(
      join(context.releases, `${binaries.at(-1)}-${context.target}.tar.gz`),
      "corrupted",
    );
    expect(context.run("latest").exitCode).not.toBe(0);
    for (const binary of binaries) {
      expect(await readFile(join(context.root, "install", binary), "utf8")).toBe("original binary");
    }
  });

  test(`${script} rejects a different signed version`, async () => {
    const context = await fixture(script);
    expect(context.run("v1.2.4").exitCode).not.toBe(0);
    expect(await Bun.file(join(context.root, "install", binaries[0])).exists()).toBe(false);
  });
}
