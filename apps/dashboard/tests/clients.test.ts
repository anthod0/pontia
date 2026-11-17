import { describe, expect, it } from "vitest";
import { clientControlDetails, defaultClientType } from "../src/clients";
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
});
