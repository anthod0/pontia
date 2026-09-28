import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { get } from "svelte/store";
import { beforeEach, expect, test, vi } from "vitest";
import WorkflowVersions from "../src/pages/workflows/WorkflowVersions.svelte";
import {
  createRevisionReader,
  revisionSelection,
  workflowRevisions,
} from "../src/pages/workflows/revisions";
import type { WorkflowGraphRevisionView } from "../src/api/types";

const mocks = vi.hoisted(() => ({ getWorkflowRevision: vi.fn(), navigate: vi.fn() }));
vi.mock("../src/api/client", () => ({ getWorkflowRevision: mocks.getWorkflowRevision }));
vi.mock("$lib/navigation", () => ({ navigate: mocks.navigate }));
const detail = (revision = 1): WorkflowGraphRevisionView => ({
  workflow_id: "wf",
  revision,
  current: false,
  nodes: [],
});
beforeEach(() => {
  vi.clearAllMocks();
  mocks.getWorkflowRevision.mockResolvedValue(detail());
});

test("revision numbers come from the accepted graph, not patch outcomes; malformed links are rejected", () => {
  expect(workflowRevisions(1)).toEqual([1]);
  expect(workflowRevisions(3)).toEqual([3, 2, 1]);
  expect(revisionSelection(null, 3)).toBe(3);
  expect(revisionSelection("2", 3)).toBe(2);
  for (const raw of ["0", "-1", "4", "1.5", "abc", "", "01", "9007199254740992"])
    expect(revisionSelection(raw, 3)).toBeNull();
});

test("late responses and failures cannot overwrite a new workflow/revision or a cancelled reader", async () => {
  let resolve!: (value: WorkflowGraphRevisionView) => void;
  mocks.getWorkflowRevision.mockImplementationOnce(
    () =>
      new Promise((r) => {
        resolve = r;
      }),
  );
  const reader = createRevisionReader();
  const first = reader.load("old", 1);
  await reader.load("new", 2);
  resolve(detail(1));
  await first;
  expect(get(reader).detail).toEqual(detail());
  expect(mocks.getWorkflowRevision.mock.calls[0][2].signal.aborted).toBe(true);
  let reject!: (error: Error) => void;
  mocks.getWorkflowRevision.mockImplementationOnce(
    () =>
      new Promise((_, r) => {
        reject = r;
      }),
  );
  const pending = reader.load("new", 3);
  reader.cancel();
  reject(new Error("late"));
  await pending;
  expect(get(reader).error).toBeNull();
});

test("empty definitions, errors and retry are explicit; only selected revision is fetched", async () => {
  mocks.getWorkflowRevision.mockRejectedValueOnce(new Error("Not found"));
  render(WorkflowVersions, { workflowId: "wf", revision: 2, onphase: vi.fn() });
  expect(await screen.findByText("Not found")).toBeInTheDocument();
  await fireEvent.click(screen.getByRole("button", { name: "Retry revision" }));
  expect(await screen.findByText("No nodes in this revision")).toBeInTheDocument();
  expect(mocks.getWorkflowRevision).toHaveBeenCalledTimes(2);
  expect(mocks.getWorkflowRevision.mock.calls.every((call) => call[1] === 2)).toBe(true);
});

test("history is read-only, preserves session navigation, and phase changes do not reload history", async () => {
  mocks.getWorkflowRevision.mockResolvedValue({
    ...detail(2),
    nodes: [
      {
        node_id: "old-node",
        parent_node_id: null,
        node_type: "agent",
        session_id: "session",
        turn_ids: [],
        phase: "Build",
        title: "Old writer",
        instructions: "Write carefully",
        inputs: ["input.md"],
        output: "output.md",
        execution_profile_id: "writer",
        execution_profile_version: "1",
        introduced_revision: 1,
        retired_revision: 3,
      },
    ],
  });
  const onphase = vi.fn();
  const view = render(WorkflowVersions, { workflowId: "wf", revision: 2, onphase });
  expect(await screen.findByText("Old writer")).toBeInTheDocument();
  expect(screen.getByText("Historical")).toBeInTheDocument();
  expect(screen.getByText("Read-only")).toBeInTheDocument();
  expect(screen.getByText("Removed in v3")).toBeInTheDocument();
  expect(screen.queryByText("pending")).not.toBeInTheDocument();
  await fireEvent.click(screen.getByText("Definition details"));
  expect(await screen.findByText("Write carefully")).toBeInTheDocument();
  await fireEvent.click(screen.getByText("Open chat →"));
  expect(mocks.navigate).toHaveBeenCalledWith("/chat/session");
  await view.rerender({ workflowId: "wf", revision: 2, onphase, requestedPhase: "1" });
  expect(screen.getByText("Viewing v2")).toBeInTheDocument();
  expect(mocks.getWorkflowRevision).toHaveBeenCalledTimes(1);
  await fireEvent.click(screen.getByRole("button", { name: "1 Build" }));
  expect(onphase).toHaveBeenCalledWith(1);
});

test("invalid links do not fetch a definition and loading is visible", async () => {
  const view = render(WorkflowVersions, { workflowId: "wf", revision: null, onphase: vi.fn() });
  expect(screen.getByText("Invalid or unavailable revision")).toBeInTheDocument();
  expect(mocks.getWorkflowRevision).not.toHaveBeenCalled();
  mocks.getWorkflowRevision.mockImplementation(() => new Promise(() => {}));
  await view.rerender({ workflowId: "wf", revision: 1, onphase: vi.fn() });
  await waitFor(() =>
    expect(screen.getByRole("status", { name: "Loading revision" })).toBeInTheDocument(),
  );
});
