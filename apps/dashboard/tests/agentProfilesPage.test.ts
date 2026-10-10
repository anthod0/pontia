import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import AgentProfilesPage from "../src/pages/AgentProfilesPage.svelte";
import type { AgentProfileView } from "../src/api/types";

const mocks = vi.hoisted(() => {
  let profiles: AgentProfileView[] = [];
  let versions: AgentProfileView[] = [];
  const mutation = () => ({ isPending: false, mutateAsync: vi.fn() });
  return {
    setProfiles(value: AgentProfileView[]) {
      profiles = value;
    },
    setVersions(value: AgentProfileView[]) {
      versions = value;
    },
    profilesQuery: {
      get data() {
        return profiles;
      },
      isPending: false,
      error: null,
      refetch: vi.fn(),
    },
    versionsQuery: {
      get data() {
        return versions;
      },
      isFetching: false,
      error: null,
      refetch: vi.fn(),
    },
    createProfileMutation: mutation(),
    createVersionMutation: mutation(),
    updateVersionMutation: mutation(),
    deleteProfileMutation: mutation(),
    deleteVersionMutation: mutation(),
  };
});

vi.mock("../src/queries/agentProfiles", () => ({
  createAgentProfilesQuery: () => mocks.profilesQuery,
  createAgentProfileVersionsQuery: () => mocks.versionsQuery,
  createAgentProfileMutation: () => mocks.createProfileMutation,
  createAgentProfileVersionMutation: () => mocks.createVersionMutation,
  updateAgentProfileVersionMutation: () => mocks.updateVersionMutation,
  deleteAgentProfileMutation: () => mocks.deleteProfileMutation,
  deleteAgentProfileVersionMutation: () => mocks.deleteVersionMutation,
}));

const profile = (overrides: Partial<AgentProfileView> = {}): AgentProfileView => ({
  profile_id: "executor",
  version: "1.0.0",
  name: "Executor",
  description: "Runs coding tasks",
  agent_kind: "executor",
  supported_client_types: ["pi"],
  default_session_role: "executor",
  handle_prefix: null,
  default_session_description: null,
  system_prompt_template: null,
  turn_prompt_template: null,
  expected_output_schema: null,
  artifact_contract: {},
  default_execution_policy: {},
  default_review_policy: {},
  metadata: {},
  active: true,
  created_at: "2026-05-14T00:00:00Z",
  updated_at: "2026-05-14T00:00:00Z",
  ...overrides,
});

beforeEach(() => {
  mocks.setProfiles([]);
  mocks.setVersions([]);
  vi.clearAllMocks();
});

afterEach(() => {
  document.body.style.pointerEvents = "";
});

test("uses alert dialog instead of window confirm for destructive profile version actions", async () => {
  const user = userEvent.setup();
  const activeProfile = profile();
  const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
  mocks.setProfiles([activeProfile]);
  mocks.setVersions([activeProfile]);

  render(AgentProfilesPage);

  await user.click(screen.getByRole("button", { name: /Delete version/i }));

  expect(confirmSpy).not.toHaveBeenCalled();
  expect(screen.getByRole("alertdialog", { name: "Archive profile version?" })).toBeInTheDocument();
  confirmSpy.mockRestore();
});

test("uses alert dialog instead of window confirm for destructive profile actions", async () => {
  const user = userEvent.setup();
  const activeProfile = profile();
  const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
  mocks.setProfiles([activeProfile]);
  mocks.setVersions([activeProfile]);

  render(AgentProfilesPage);

  await user.click(screen.getByRole("button", { name: /Delete profile/i }));

  expect(confirmSpy).not.toHaveBeenCalled();
  expect(screen.getByRole("alertdialog", { name: "Archive profile?" })).toBeInTheDocument();
  confirmSpy.mockRestore();
});

test("archives the selected profile through the profile mutation", async () => {
  const user = userEvent.setup();
  const activeProfile = profile();
  mocks.setProfiles([activeProfile]);
  mocks.setVersions([activeProfile]);
  mocks.deleteProfileMutation.mutateAsync.mockResolvedValue({
    profile_id: activeProfile.profile_id,
    archived_versions: 1,
  });

  render(AgentProfilesPage);

  await user.click(screen.getByRole("button", { name: /Delete profile/i }));
  await user.click(screen.getByRole("button", { name: "Archive" }));

  expect(mocks.deleteProfileMutation.mutateAsync).toHaveBeenCalledWith("executor");
});
