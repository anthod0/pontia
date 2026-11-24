import { expect, test } from "vitest";
import {
  buildAgentProfileInput,
  createAgentProfileDraft,
  createAgentProfileDraftFromProfile,
  profileIdFromName,
} from "../src/pages/agentProfiles/form";

const launch = { client_type: "pi", client_key: "pi", model: "provider/model" };

test("creates a profile with its client and model and optional fields unset", () => {
  expect(
    buildAgentProfileInput(
      { ...createAgentProfileDraft(), ...launch, profile_id: " coder ", name: " Coder " },
      { creating: true },
    ),
  ).toEqual({
    ok: true,
    input: {
      profile_id: "coder",
      name: "Coder",
      description: null,
      ...launch,
      reasoning_effort: null,
      use_yolo: false,
      system_prompt: null,
    },
  });
});

test("preserves an existing profile's ID and launch settings while editing instructions", () => {
  const draft = createAgentProfileDraftFromProfile({
    profile_id: "Old Coder",
    name: "Coder",
    description: null,
    ...launch,
    reasoning_effort: "high",
    use_yolo: true,
    system_prompt: "original",
    created_at: "2026-11-20",
    updated_at: "2026-11-20",
  });
  draft.system_prompt = "Updated instructions";
  expect(buildAgentProfileInput(draft, { creating: false })).toEqual({
    ok: true,
    input: {
      profile_id: "Old Coder",
      name: "Coder",
      description: null,
      ...launch,
      reasoning_effort: "high",
      use_yolo: true,
      system_prompt: "Updated instructions",
    },
  });
});

test.each([
  { profile_id: "", name: "Coder" },
  { profile_id: "coder", name: " " },
  { profile_id: "Coder" },
  { profile_id: "-coder" },
  { profile_id: "code review" },
  { client_type: "", client_key: "", model: "" },
  { model: "" },
  { system_prompt: "Use {{input}}" },
])("rejects a new profile with %j", (invalid) => {
  expect(
    buildAgentProfileInput(
      { ...createAgentProfileDraft(), ...launch, profile_id: "coder", name: "Coder", ...invalid },
      { creating: true },
    ).ok,
  ).toBe(false);
});

test.each([
  ["Release notes writer", "release-notes-writer"],
  ["  Codex: default model!  ", "codex-default-model"],
  ["评审助手", ""],
])("suggests a Profile ID for %j", (name, id) => {
  expect(profileIdFromName(name)).toBe(id);
});
