import { describe, expect, it } from "vitest";
import {
  clientControlDetails,
  clientSessionLabel,
  dashboardClient,
  sessionClientKey,
  defaultClientType,
} from "../src/clients";
import type { SessionView } from "../src/api/types";

const session = (client_type: string, extra = {}): SessionView =>
  ({ client_type, ...extra }) as SessionView;

describe("registered dashboard clients", () => {
  it("uses pi as the default client for creation", () => {
    expect(defaultClientType).toBe("pi");
  });
  it("interprets the existing native detail extension without changing its content", () => {
    const details = {
      connection: "reconciling",
      profile: {
        profile_id: "executor",
        version: "2",
        status: "unverified",
        error: "instructions are not verified",
      },
    };
    expect(clientControlDetails(session("codex", { codex: details }))).toEqual(details);
    expect(clientControlDetails(session("codex"))).toBeNull();
    expect(clientControlDetails(session("pi", { codex: details }))).toBeNull();
  });
  it("labels an ACP session with the configured agent name only", () => {
    const acp = {
      agent_id: "antigravity",
      name: "Antigravity",
      agent_info: { name: "agy", title: "Antigravity ACP Server", version: "1.5.16" },
    };
    expect(clientSessionLabel(session("acp", { acp }))).toBe("Antigravity");
    expect(clientSessionLabel(session("acp", { acp: { agent_id: "custom" } }))).toBe("custom");
  });
  it("resolves the icon target and instruction status only for the owning client", () => {
    const acp = { agent_id: "grok", system_prompt_injected: false };
    for (const [type, key, status] of [
      ["acp", "grok", false],
      ["pi", undefined, undefined],
      ["unknown", undefined, undefined],
    ] as const) {
      const view = session(type, { acp });
      expect(sessionClientKey(view)).toBe(key);
      expect(dashboardClient(type)?.instructionsInjected?.(view)).toBe(status);
    }
    expect(dashboardClient("acp")?.iconKey("grok")).toBe("grok");
    expect(dashboardClient("acp")?.iconKey("custom")).toBeUndefined();
  });
  it("maps ACP connection diagnostics into the shared dashboard status", () => {
    expect(
      clientControlDetails(
        session("acp", {
          acp: {
            connection: "reconnecting",
            connection_error: "native protocol stream closed",
          },
        }),
      ),
    ).toMatchObject({
      connection: "reconnecting",
      refreshWhileActive: true,
      diagnostic: { kind: "reconnecting", message: "native protocol stream closed" },
    });
    expect(
      clientControlDetails(
        session("acp", {
          acp: { connection: "failed", connection_error: "agent executable was not found" },
        }),
      )?.diagnostic,
    ).toEqual({ kind: "error", message: "agent executable was not found" });
  });
});
