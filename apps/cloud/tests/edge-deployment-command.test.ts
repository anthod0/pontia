import { expect, test } from "bun:test";
import { deploymentModeCommand } from "../src/lib/edge-deployment-command";
import { deploymentCommand } from "../src/lib/server/edge-deployment";
import unsafePorts from "../../../config/edge-unsafe-ports.json";

const command = deploymentCommand("https://pontia.example", "edge-id", "deployment-ticket");

test("default mode preserves the existing HTTP-01 command regardless of custom port input", () => {
  for (const port of [undefined, 0, 25, 8443]) {
    expect(deploymentModeCommand(command, "default", port)).toBe(command);
  }
  expect(command).not.toContain("--port");
  expect(command).not.toContain("--acme-challenge");
});

test("custom port mode uses DNS-01 and preserves the deployment identity and ticket", () => {
  for (const port of [80, 443, 8443, 65535]) {
    expect(deploymentModeCommand(command, "custom", port)).toBe(
      `${command} \\\n  --port ${port} \\\n  --acme-challenge dns-01`,
    );
  }
});

test("invalid or prohibited custom ports produce no copyable command", () => {
  for (const port of [undefined, NaN, Infinity, 0, -1, 65536, 8443.5, ...unsafePorts]) {
    expect(deploymentModeCommand(command, "custom", port)).toBeNull();
  }
});
