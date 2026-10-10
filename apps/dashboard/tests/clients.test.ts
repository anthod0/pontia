import { describe, expect, it } from "vitest";
import {
  clientControlDetails,
  clientDeliveryPolicy,
  creationClientTypes,
  defaultClientType,
  profileClientTypes,
  workspaceClientTypes,
} from "../src/clients";
import type { SessionView } from "../src/api/types";

const session = (client_type: string, extra = {}): SessionView =>
  ({ client_type, ...extra }) as SessionView;

describe("registered dashboard clients", () => {
  it("preserves the default and the client choices available to each creation flow", () => {
    expect(defaultClientType).toBe("pi");
    expect(creationClientTypes).toEqual(["pi", "codex"]);
    expect(workspaceClientTypes).toEqual(["pi"]);
    expect(profileClientTypes).toEqual(["pi"]);
  });
  it("uses client-owned input delivery semantics", () => {
    expect(clientDeliveryPolicy(session("codex"))).toBe("steer");
    expect(clientDeliveryPolicy(session("pi"))).toBe("after_idle");
    expect(clientDeliveryPolicy(session("unregistered"))).toBe("after_idle");
    expect(clientDeliveryPolicy(null)).toBe("after_idle");
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
});
