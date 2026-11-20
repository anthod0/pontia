import { expect, test } from "vitest";
import {
  buildAgentProfileInput,
  createAgentProfileDraft,
  createAgentProfileDraftFromProfile,
} from "../src/pages/agentProfiles/form";

test("creates a profile with optional fields unset", () => {
  expect(
    buildAgentProfileInput({
      ...createAgentProfileDraft(),
      profile_id: " coder ",
      name: " Coder ",
    }),
  ).toEqual({
    ok: true,
    input: {
      profile_id: "coder",
      name: "Coder",
      description: null,
      client_type: null,
      client_key: null,
      model: null,
      system_prompt: null,
    },
  });
});

test("preserves launch defaults while editing instructions", () => {
  const draft = createAgentProfileDraftFromProfile({
    profile_id: "coder",
    name: "Coder",
    description: null,
    client_type: "pi",
    client_key: "pi",
    model: "provider/model",
    system_prompt: "original",
    created_at: "2026-11-20",
    updated_at: "2026-11-20",
  });
  draft.system_prompt = "Updated instructions";
  expect(buildAgentProfileInput(draft)).toEqual({
    ok: true,
    input: {
      profile_id: "coder",
      name: "Coder",
      description: null,
      client_type: "pi",
      client_key: "pi",
      model: "provider/model",
      system_prompt: "Updated instructions",
    },
  });
});

test.each([
  { profile_id: "", name: "Coder" },
  { profile_id: "coder", name: " " },
  { client_type: "pi", client_key: "pi", model: "" },
  { client_type: "", client_key: "", model: "model" },
  { system_prompt: "Use {{input}}" },
])("rejects incomplete identifiers, launch selections and template instructions: %j", (invalid) => {
  expect(
    buildAgentProfileInput({
      ...createAgentProfileDraft(),
      profile_id: "coder",
      name: "Coder",
      ...invalid,
    }).ok,
  ).toBe(false);
});
