import { testDevice } from "./deviceFixture";
import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, expect, test, vi } from "vitest";
import WorkflowDetailPage from "../src/pages/WorkflowDetailPage.svelte";
const { queryClient } = testDevice;
import type { WorkflowDetailView } from "../src/api/types";
const mocks = vi.hoisted(() => ({
  getWorkflow: vi.fn(),
}));
vi.mock("../src/api/client", async (importOriginal) => {
  const original = await importOriginal();
  const mocked = {
    ...mocks,
    listWorkflows: vi.fn(),
    pauseWorkflow: vi.fn(),
    resumeWorkflow: vi.fn(),
  };
  return {
    ...original,
    ...mocked,
    createApi: (device) => ({ ...original.createApi(device), ...mocked }),
  };
});
vi.mock("$lib/navigation", () => ({
  navigate: async (
    path: string,
    query: Record<string, string | null> = {},
    options: { replaceState?: boolean } = {},
  ) => {
    const url = new URL(path, window.location.origin);
    for (const [key, value] of Object.entries(query))
      if (value !== null) url.searchParams.set(key, value);
    window.history[options.replaceState ? "replaceState" : "pushState"]({}, "", url);
    window.dispatchEvent(new PopStateEvent("popstate"));
  },
}));
const snapshot: WorkflowDetailView = {
  retry_failure_event_id: null,
  retry_unavailable_reason: null,
  recoveries: [],
  workflow_id: "wf",
  title: "Example",
  state: "completed",
  initiator_session_id: "origin",
  termination_reason: null,
  failure_message: null,
  cwd: "/workspace",
  agent_submitted_count: 0,
  agent_total_count: 1,
  current_node_id: null,
  started_at: null,
  completed_at: null,
  created_at: "",
  updated_at: "",
  elapsed_ms: 0,
  nodes: [
    {
      node_id: "n",
      phase: "Build",
      title: "Current writer",
      status: "pending",
      session_id: "session",
      session_state: null,
      submitted_at: null,
    },
  ],
};
beforeEach(() => {
  vi.clearAllMocks();
  queryClient.clear();
  mocks.getWorkflow.mockResolvedValue(snapshot);
});
function visit(query: string) {
  window.history.replaceState({}, "", `/workflows/wf${query}`);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

test("opens the initiating Session from a Workflow", async () => {
  visit("");
  render(WorkflowDetailPage, { routeWorkflowId: "wf" });
  await fireEvent.click(await screen.findByRole("button", { name: /origin/ }));
  expect(window.location.pathname).toBe("/chat/origin");
});
