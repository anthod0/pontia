import { translate, type MessageKey } from "$lib/i18n";
import { useDeviceRuntime } from "../../device/context";
import { onMount } from "svelte";
import { toast } from "svelte-sonner";
import { isTransientNetworkError } from "../../api/client";
import type { AgentClientView, AgentClientSetupOperation } from "../../api/types";

export type ClientStatusTone = "busy" | "error" | "inactive" | "warning" | "off" | "success";

/** One status per client: problems first, then whether it is enabled. */
export function clientStatus(
  client: AgentClientView,
  operation: AgentClientSetupOperation | null,
): { tone: ClientStatusTone; label: MessageKey } {
  if (operation?.status === "running") return { tone: "busy", label: "clients.settingUp" };
  if (operation?.status === "failed") return { tone: "error", label: "clients.status.setupFailed" };
  if (client.detection.status === "not_detected")
    return { tone: "inactive", label: "clients.detection.not_detected" };
  if (client.detection.status === "unknown")
    return { tone: "warning", label: "clients.status.detectionUnknown" };
  if (client.setup.status === "required")
    return { tone: "warning", label: "clients.status.setupRequired" };
  if (client.setup.status === "unknown")
    return { tone: "warning", label: "clients.status.setupUnknown" };
  return client.enabled
    ? { tone: "success", label: "common.enabled" }
    : { tone: "off", label: "common.disabled" };
}

export function createAgentClients() {
  const device = useDeviceRuntime();
  const {
    listAgentClients,
    patchAgentClient,
    resetAgentClient,
    setupAgentClient,
    getAgentClientSetup,
  } = device.api;
  const profiles = device.agentProfileQueries.createAgentProfilesQuery();

  let clients = $state<AgentClientView[]>([]);
  let selectedId = $state<string | null>(null);
  let error = $state("");
  let loadFailed = $state(false);
  let refreshing = $state(false);
  let saving = $state<string[]>([]);
  let startingSetup = $state<string | null>(null);
  let setupOperation = $state<AgentClientSetupOperation | null>(null);
  let setupConnectionError = $state("");
  let setupLoading = $state(true);
  let loading = $state(true);
  const groups = $derived(
    (
      [
        ["detected", clients.filter((client) => client.detection.status !== "not_detected")],
        ["not_detected", clients.filter((client) => client.detection.status === "not_detected")],
      ] as const
    ).filter(([, list]) => list.length > 0),
  );
  const ordered = $derived(groups.flatMap(([, list]) => list));
  const selected = $derived(
    ordered.find((client) => client.id === selectedId) ?? ordered[0] ?? null,
  );
  const setupBusy = $derived(startingSetup !== null || setupOperation?.status === "running");

  function operationFor(client: AgentClientView): AgentClientSetupOperation | null {
    if (startingSetup === client.id && setupOperation?.status !== "running")
      return {
        id: "",
        client_type: client.client_type,
        client_key: client.client_key,
        status: "running",
        error: null,
        output: "",
      };
    return setupOperation?.client_type === client.client_type &&
      setupOperation.client_key === client.client_key
      ? setupOperation
      : null;
  }

  function profileCount(client: AgentClientView): number {
    return (profiles.data ?? []).filter(
      (profile) =>
        profile.client_type === client.client_type && profile.client_key === client.client_key,
    ).length;
  }

  function select(client: AgentClientView) {
    selectedId = client.id;
    error = "";
  }

  async function load() {
    clients = await listAgentClients();
    loadFailed = false;
  }

  async function refresh() {
    refreshing = true;
    error = "";
    try {
      await load();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      loadFailed = clients.length === 0;
    } finally {
      refreshing = false;
    }
  }

  onMount(() => {
    load()
      .catch((cause) => {
        error = cause instanceof Error ? cause.message : String(cause);
        loadFailed = true;
      })
      .finally(() => (loading = false));
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function pollSetup() {
      try {
        const operation = await getAgentClientSetup({ signal: controller.signal });
        if (controller.signal.aborted) return;
        const completed =
          operation &&
          operation.status !== "running" &&
          (setupOperation?.id !== operation.id || setupOperation.status === "running");
        setupOperation = operation;
        setupConnectionError = "";
        if (completed) await load();
      } catch (cause) {
        if (!controller.signal.aborted)
          setupConnectionError = cause instanceof Error ? cause.message : String(cause);
      } finally {
        if (!controller.signal.aborted) {
          setupLoading = false;
          timer = setTimeout(pollSetup, 1500);
        }
      }
    }
    void pollSetup();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  });

  async function setup(client: AgentClientView) {
    startingSetup = client.id;
    error = "";
    try {
      setupOperation = await setupAgentClient(client);
      setupConnectionError = "";
      if (setupOperation.status !== "running") await load();
    } catch (cause) {
      if (isTransientNetworkError(cause)) setupConnectionError = String(cause);
      else error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      startingSetup = null;
    }
  }

  async function perform(client: AgentClientView, action: () => Promise<void>) {
    saving = [...saving, client.id];
    error = "";
    try {
      await action();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = saving.filter((id) => id !== client.id);
    }
  }

  function toggle(client: AgentClientView) {
    return perform(client, async () => {
      const saved = await patchAgentClient(client, { enabled: !client.enabled });
      clients = clients.map((entry) => (entry.id === saved.id ? saved : entry));
      toast.success(saved.enabled ? translate("clients.enabled") : translate("clients.disabled"), {
        description: translate("clients.sessionsUnchanged"),
      });
    });
  }

  function reset(client: AgentClientView) {
    return perform(client, async () => {
      await resetAgentClient(client);
      await load();
      toast.success(translate("clients.restored"));
    });
  }
  return {
    get clients() {
      return clients;
    },
    get groups() {
      return groups;
    },
    get selected() {
      return selected;
    },
    get error() {
      return error;
    },
    get loadFailed() {
      return loadFailed;
    },
    get refreshing() {
      return refreshing;
    },
    get setupBusy() {
      return setupBusy;
    },
    get setupConnectionError() {
      return setupConnectionError;
    },
    get setupLoading() {
      return setupLoading;
    },
    get loading() {
      return loading;
    },
    saving: (client: AgentClientView) => saving.includes(client.id),
    operationFor,
    profileCount,
    select,
    refresh,
    setup,
    toggle,
    reset,
  };
}
