import { onDestroy, onMount, untrack } from "svelte";
import { fromStore } from "svelte/store";
import { toast } from "svelte-sonner";
import { translate } from "$lib/i18n";
import { navigate } from "$lib/navigation";
import { claimChatEntryAutofocus } from "$lib/chatEntryAutofocus";
import { titleFromInitialPrompt } from "$lib/session-chat/sessionChat";
import { readStored, scopedKey, writeStored } from "$lib/storage";
import { defaultClientType } from "../../clients";
import { isTransientNetworkError } from "../../api/client";
import type { DeviceRuntime } from "../../device/runtime";

export function createNewChat(device: DeviceRuntime, scope: () => string | undefined) {
  const { createSession, loadSessionDetail } = device.sessions;
  const { loadSessionTimeline, resetTimelineState } = device.timeline;
  const { chatDraft, clearChatDraft } = device.chatDraft;
  const workspaceKey = scopedKey("pontia.chat.lastWorkspaceId", device.id);
  const clientKey = scopedKey("pontia.chat.lastCreationOptionId", device.id);
  const workspaceQuery = fromStore(device.workspaceQueries.createWorkspacesStore());
  const optionsQuery = fromStore(device.sessionCreation.createSessionCreationOptionsStore());
  const workspaces = $derived(workspaceQuery.current.data ?? []);
  const creationOptions = $derived(optionsQuery.current.data ?? []);
  let createWorkspaceId = $state("");
  let creationOptionId = $state(readStored(clientKey) ?? defaultClientType);
  let reasoningEffort = $state("");
  let model = $state("");
  let creating = $state(false);
  let mounted = true;
  onDestroy(() => {
    mounted = false;
  });
  let actionError = $state<string | null>(null);
  let onboarding = $state(false);
  let deciding = $state(true);
  let autofocus = $state(false);
  let queryId = $state(new URLSearchParams(window.location.search).get("workspace"));
  onMount(() => {
    const sync = () => (queryId = new URLSearchParams(window.location.search).get("workspace"));
    window.addEventListener("popstate", sync);
    return () => window.removeEventListener("popstate", sync);
  });
  function selectWorkspace(id: string, remember = true) {
    if (id === createWorkspaceId) return;
    createWorkspaceId = id;
    model = "";
    reasoningEffort = "";
    if (remember) writeStored(workspaceKey, id);
  }
  function selectClient(id: string, nextModel = "", effort = "") {
    creationOptionId = id;
    model = nextModel;
    reasoningEffort = effort;
  }
  $effect(() => {
    const list = workspaces,
      query = queryId;
    if (!list.length) return;
    untrack(() => {
      const id =
        [query, createWorkspaceId, readStored(workspaceKey)].find((id) =>
          list.some((w) => w.workspace_id === id),
        ) ?? list[0].workspace_id;
      selectWorkspace(id, id !== query);
    });
  });
  $effect(() => {
    if (deciding && !workspaceQuery.current.isPending && !workspaceQuery.current.isFetching) {
      onboarding = !workspaces.length && !workspaceQuery.current.error;
      autofocus = Boolean(workspaces.length && claimChatEntryAutofocus(device.id + ":/"));
      deciding = false;
    }
  });
  const selectedCreationOption = $derived(creationOptions.find((o) => o.id === creationOptionId));
  $effect(() => {
    if (selectedCreationOption) writeStored(clientKey, selectedCreationOption.id);
    else if (creationOptions.length) selectClient(creationOptions[0].id);
  });
  const canCreate = $derived(
    Boolean(
      createWorkspaceId && selectedCreationOption && !optionsQuery.current.isError && !creating,
    ),
  );
  const error = $derived(
    actionError ??
      optionsQuery.current.error?.message ??
      (workspaceQuery.current.error &&
      !isTransientNetworkError(workspaceQuery.current.error.message)
        ? workspaceQuery.current.error.message
        : null),
  );
  let toasted: string | null = null;
  $effect(() => {
    if (error && error !== toasted)
      toast.error(
        translate(
          actionError
            ? "chat.startFailed"
            : optionsQuery.current.isError
              ? "clients.loadFailed"
              : "workspaces.loadFailed",
        ),
        {
          description: error,
          ...(!actionError
            ? {
                action: {
                  label: translate("common.retry"),
                  onClick: () => {
                    toasted = null;
                    if (optionsQuery.current.isError) void optionsQuery.current.refetch();
                    else {
                      deciding = true;
                      void workspaceQuery.current.refetch();
                    }
                  },
                },
              }
            : {}),
        },
      );
    toasted = error;
  });
  async function startChat(initialPrompt: string, replaceEntry = false): Promise<boolean> {
    if (!canCreate || !selectedCreationOption || !initialPrompt.trim()) return false;
    const workspaceId = createWorkspaceId;
    creating = true;
    actionError = null;
    try {
      const result = await createSession({
        client_type: selectedCreationOption.client_type,
        ...(selectedCreationOption.model_catalog_key && model ? { model } : {}),
        ...(reasoningEffort ? { reasoning_effort: reasoningEffort } : {}),
        workspace_id: workspaceId,
        title: titleFromInitialPrompt(initialPrompt),
        include_standalone_session_instructions: true,
        initial_task: { input: initialPrompt, metadata: { source: "dashboard_chat" } },
        metadata: {
          ...selectedCreationOption.metadata,
          source: "dashboard_chat",
          title_source: "input",
        },
      });
      if (!mounted) return true;
      writeStored(workspaceKey, workspaceId);
      clearChatDraft();
      resetTimelineState(result.session.session_id);
      if (scope() === undefined && !replaceEntry)
        void navigate(`/chat/${result.session.session_id}`);
      else
        void navigate(`/chat/${result.session.session_id}`, undefined, {
          scope: scope(),
          ...(replaceEntry ? { replaceState: true } : {}),
        });
      void Promise.all([
        loadSessionDetail(result.session.session_id),
        loadSessionTimeline(result.session.session_id, {
          mode: "rebuild",
          ...(result.session.capabilities.topology === true ? { topology: true } : {}),
        }),
      ]).catch((error) => {
        actionError = error instanceof Error ? error.message : String(error);
      });
      return true;
    } catch (error) {
      actionError = error instanceof Error ? error.message : String(error);
      return false;
    } finally {
      creating = false;
    }
  }

  return {
    chatDraft,
    startChat,
    selectClient,
    get workspaceId() {
      return createWorkspaceId;
    },
    set workspaceId(id: string) {
      selectWorkspace(id);
    },
    get creationOptionId() {
      return creationOptionId;
    },
    set creationOptionId(id: string) {
      selectClient(id);
    },
    get model() {
      return model;
    },
    set model(value: string) {
      model = value;
    },
    get reasoningEffort() {
      return reasoningEffort;
    },
    set reasoningEffort(value: string) {
      reasoningEffort = value;
    },
    get creating() {
      return creating;
    },
    get canCreate() {
      return canCreate;
    },
    get autofocus() {
      return autofocus;
    },
    get workspaces() {
      return workspaces;
    },
    get workspacesLoading() {
      return workspaceQuery.current.isFetching;
    },
    get selectedWorkspace() {
      return workspaces.find((w) => w.workspace_id === createWorkspaceId) ?? null;
    },
    get creationOptions() {
      return creationOptions;
    },
    get selectedCreationOption() {
      return selectedCreationOption;
    },
    get loading() {
      return !workspaces.length && (workspaceQuery.current.isPending || deciding);
    },
    get onboarding() {
      return onboarding;
    },
    completeOnboarding() {
      onboarding = false;
    },
  };
}
export type NewChatController = ReturnType<typeof createNewChat>;
