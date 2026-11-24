import { translate } from "$lib/i18n";
import { useDeviceRuntime } from "../../device/context";
import { onMount } from "svelte";
import { toast } from "svelte-sonner";
import { isTransientNetworkError } from "../../api/client";
import type { AgentClientView, AgentClientSetupOperation } from "../../api/types";

export function createAgentClients() {
  const device = useDeviceRuntime();
  const {
    listAgentClients,
    patchAgentClient,
    resetAgentClient,
    setupAgentClient,
    getAgentClientSetup,
  } = device.api;

  let clients = $state<AgentClientView[]>([]);
  let selected = $state<AgentClientView | null>(null);
  let error = $state("");
  let saving = $state(false);
  let startingSetup = $state(false);
  let setupOperation = $state<AgentClientSetupOperation | null>(null);
  let setupConnectionError = $state("");
  let setupLoading = $state(true);
  let busy = $derived(saving || startingSetup || setupOperation?.status === "running");
  let selectedSetup = $derived(
    setupOperation?.client_type === selected?.client_type &&
      setupOperation?.client_key === selected?.client_key
      ? setupOperation
      : null,
  );
  let loading = $state(true);

  function select(client: AgentClientView) {
    selected = client;
    error = "";
  }

  async function refresh() {
    clients = await listAgentClients();
    if (selected) selected = clients.find((client) => client.id === selected?.id) ?? null;
  }

  onMount(() => {
    refresh()
      .catch((cause) => (error = String(cause)))
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
        if (completed) await refresh();
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
    startingSetup = true;
    error = "";
    try {
      setupOperation = await setupAgentClient(client);
      setupConnectionError = "";
      if (setupOperation.status !== "running") await refresh();
    } catch (cause) {
      if (isTransientNetworkError(cause)) setupConnectionError = String(cause);
      else error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      startingSetup = false;
    }
  }

  async function perform(action: () => Promise<void>) {
    saving = true;
    error = "";
    try {
      await action();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      saving = false;
    }
  }

  function toggle(client: AgentClientView) {
    return perform(async () => {
      const saved = await patchAgentClient(client, { enabled: !client.enabled });
      clients = clients.map((entry) => (entry.id === saved.id ? saved : entry));
      selected = saved;
      toast.success(saved.enabled ? translate("clients.enabled") : translate("clients.disabled"), {
        description: translate("clients.sessionsUnchanged"),
      });
    });
  }

  function reset(client: AgentClientView) {
    return perform(async () => {
      await resetAgentClient(client);
      await refresh();
      toast.success(translate("clients.restored"));
    });
  }
  return {
    get clients() {
      return clients;
    },
    get selected() {
      return selected;
    },
    get error() {
      return error;
    },
    get saving() {
      return saving;
    },
    get startingSetup() {
      return startingSetup;
    },
    get setupConnectionError() {
      return setupConnectionError;
    },
    get setupLoading() {
      return setupLoading;
    },
    get busy() {
      return busy;
    },
    get selectedSetup() {
      return selectedSetup;
    },
    get loading() {
      return loading;
    },
    select,
    refresh,
    setup,
    perform,
    toggle,
    reset,
  };
}
