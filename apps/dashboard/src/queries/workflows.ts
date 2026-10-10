import { createQuery, queryOptions } from "@tanstack/svelte-query";
import { getWorkflowDocument, getWorkflowRevision, listWorkflowPatches } from "../api/client";
import { queryClient } from "./queryClient";

export const workflowKeys = {
  all: ["workflows"] as const,
  revision: (workflowId: string, revision: number | null) =>
    [...workflowKeys.all, workflowId, "revisions", revision] as const,
  patches: (workflowId: string) => [...workflowKeys.all, workflowId, "patches"] as const,
  document: (workflowId: string, ref: string | null) =>
    [...workflowKeys.all, workflowId, "documents", { ref }] as const,
};

function workflowRevisionOptions(workflowId: string, revision: number | null) {
  return queryOptions({
    queryKey: workflowKeys.revision(workflowId, revision),
    enabled: revision !== null,
    queryFn: ({ signal }) => {
      if (revision === null) throw new Error("A workflow revision is required");
      return getWorkflowRevision(workflowId, revision, { signal });
    },
    staleTime: Number.POSITIVE_INFINITY,
  });
}

function workflowPatchesOptions(workflowId: string, enabled: boolean) {
  return queryOptions({
    queryKey: workflowKeys.patches(workflowId),
    enabled,
    queryFn: ({ signal }) => listWorkflowPatches(workflowId, { signal }),
    select: (patches) =>
      [...patches].sort(
        (a, b) =>
          Date.parse(b.requested_at) - Date.parse(a.requested_at) ||
          b.patch_id.localeCompare(a.patch_id),
      ),
  });
}

function workflowDocumentOptions(workflowId: string, ref: string | null, enabled: boolean) {
  return queryOptions({
    queryKey: workflowKeys.document(workflowId, ref),
    enabled: enabled && ref !== null,
    queryFn: ({ signal }) => {
      if (ref === null) throw new Error("A workflow document reference is required");
      return getWorkflowDocument(workflowId, ref, { signal });
    },
  });
}

export function createWorkflowRevisionQuery(
  workflowId: () => string,
  revision: () => number | null,
) {
  return createQuery(
    () => workflowRevisionOptions(workflowId(), revision()),
    () => queryClient,
  );
}

export function createWorkflowPatchesQuery(
  workflowId: () => string,
  enabled: () => boolean = () => true,
) {
  return createQuery(
    () => workflowPatchesOptions(workflowId(), enabled()),
    () => queryClient,
  );
}

export function createWorkflowDocumentQuery(
  workflowId: () => string,
  ref: () => string | null,
  enabled: () => boolean,
) {
  return createQuery(
    () => workflowDocumentOptions(workflowId(), ref(), enabled()),
    () => queryClient,
  );
}
