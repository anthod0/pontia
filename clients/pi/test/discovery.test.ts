import { expect, test } from "vitest";
import { pontiaHomeFromEnv } from "../src/discovery.js";

test("resolves Pontia home from explicit settings or the supplied home", () => {
  expect(pontiaHomeFromEnv({ PONTIA_HOME: "/tmp/pontia", HOME: "/tmp/user" })).toBe("/tmp/pontia");
  expect(pontiaHomeFromEnv({ HOME: "/tmp/user" })).toBe("/tmp/user/.pontia");
});

test.each(["", "   ", "/", "relative/pontia", "~/.pontia", "/tmp/../pontia"])("rejects invalid explicit root %j", (root) => {
  expect(pontiaHomeFromEnv({ PONTIA_HOME: root, HOME: "/tmp/user" })).toBeUndefined();
});
