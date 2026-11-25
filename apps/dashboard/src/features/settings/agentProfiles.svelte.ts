import { page } from "$app/state";
import { untrack } from "svelte";
import type { AgentProfileDraft } from "../../pages/agentProfiles/form";
import { createQuery } from "@tanstack/svelte-query";
import { translate } from "$lib/i18n";
import { toast } from "svelte-sonner";
import { useDeviceRuntime } from "../../device/context";
import type { AgentProfileView } from "../../api/types";
import type { ProfileTemplate } from "../../pages/agentProfiles/templates";

export function createProfileQueries(enabled = () => true) {
  const device = useDeviceRuntime();
  const profiles = device.agentProfileQueries.createAgentProfilesQuery(enabled);
  const clients = createQuery(
    () => ({
      enabled: enabled(),
      queryKey: ["agent-clients"],
      queryFn: () => device.api.listAgentClients(),
    }),
    () => device.queryClient,
  );
  const enabledClients = $derived(clients.data?.filter((client) => client.enabled) ?? []);
  return {
    profiles,
    clients,
    get enabledClients() {
      return enabledClients;
    },
  };
}

export function createAgentProfiles(onopen?: (title: string) => void) {
  const queries = createProfileQueries();
  const { profiles, clients } = queries;
  let startDraft = $state<AgentProfileDraft>();
  let incoming = untrack(() => ({
    draft: page.state.profileDraft,
    profile: page.url.searchParams.get("profile"),
  }));
  $effect(() => {
    if ((!incoming.draft && !incoming.profile) || loading || !profiles.data) return;
    const { draft, profile } = incoming;
    incoming = { draft: undefined, profile: null };
    if (draft) {
      selectedId = "";
      startDraft = draft;
      untrack(() => onopen?.(translate("profiles.new")));
    } else {
      const target = profiles.data.find((item) => item.profile_id === profile);
      if (target) {
        selectedId = target.profile_id;
        untrack(() => onopen?.(target.name));
      }
    }
  });
  // `null` selects nothing; `''` opens the new-profile editor.
  let selectedId = $state<string | null>(null);
  let saving = $state(false);
  // A template chosen from the empty state, applied when the new-profile editor opens.
  let startTemplate = $state<ProfileTemplate>();
  let deleting = $state<AgentProfileView | null>(null);
  const selected = $derived(profiles.data?.find((profile) => profile.profile_id === selectedId));
  const editorOpen = $derived(selectedId === "" || !!selected);
  // Svelte Query only notifies about result fields that were read, so read both flags
  // instead of short-circuiting past the client query.
  const loading = $derived([profiles.isPending, clients.isPending].includes(true));
  const creationDisabled = $derived(saving || loading || !profiles.data);

  function openNew(template?: ProfileTemplate) {
    startDraft = undefined;
    startTemplate = template;
    selectedId = "";
  }

  function profileSaved(id: string | null) {
    selectedId = id;
    toast.success(translate("profiles.saved"));
  }

  function clientName(profile: AgentProfileView) {
    return (
      clients.data?.find(
        (client) =>
          client.client_type === profile.client_type && client.client_key === profile.client_key,
      )?.name ?? `${profile.client_type} / ${profile.client_key}`
    );
  }
  return {
    get selectedId() {
      return selectedId;
    },
    get selected() {
      return selected;
    },
    get saving() {
      return saving;
    },
    get startTemplate() {
      return startTemplate;
    },
    get deleting() {
      return deleting;
    },
    get editorOpen() {
      return editorOpen;
    },
    get startDraft() {
      return startDraft;
    },
    get enabledClients() {
      return queries.enabledClients;
    },
    get loading() {
      return loading;
    },
    get creationDisabled() {
      return creationDisabled;
    },
    set selectedId(value: string | null) {
      selectedId = value;
    },
    set saving(value: boolean) {
      saving = value;
    },
    set deleting(value: AgentProfileView | null) {
      deleting = value;
    },
    clients,
    profiles,
    openNew,
    clientName,
    profileSaved,
  };
}
