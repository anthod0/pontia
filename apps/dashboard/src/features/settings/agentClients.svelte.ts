import { translate, type MessageKey } from "$lib/i18n";
import { useDeviceRuntime } from "../../device/context";
import { onMount } from "svelte";
import { toast } from "svelte-sonner";
import { isMobileShell } from "$lib/hooks/shellMode.svelte";
import { ApiError } from "../../api/errors";
import { isTransientNetworkError } from "../../api/client";
import type {
  AgentClientInstallMethod,
  AgentClientOperation,
  AgentClientOperationState,
  AgentClientSetupOperation,
  AgentClientView,
} from "../../api/types";
import { readOperationStream } from "./agentClientInstall";

export type ClientStatusTone = "busy" | "error" | "inactive" | "warning" | "off" | "success";

/** One status per client: problems first, then whether it is enabled. */
export function clientStatus(
  client: AgentClientView,
  setup: AgentClientSetupOperation | null,
  install: AgentClientOperation | null,
): { tone: ClientStatusTone; label: MessageKey } {
  if (install?.status === "running")
    return {
      tone: "busy",
      label: install.cancel_requested
        ? "clients.install.status.cancelling"
        : install.phase === "setting_up"
          ? "clients.settingUp"
          : "clients.install.status.installing",
    };
  if (install?.status === "failed")
    return { tone: "error", label: "clients.install.status.failed" };
  if (setup?.status === "running") return { tone: "busy", label: "clients.settingUp" };
  if (setup?.status === "failed") return { tone: "error", label: "clients.status.setupFailed" };
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
    installAgentClient,
    getLatestAgentClientOperation,
    getAgentClientOperation,
    cancelAgentClientOperation,
    streamAgentClientOperation,
  } = device.api;
  const profiles = device.agentProfileQueries.createAgentProfilesQuery();

  let clients = $state<AgentClientView[]>([]);
  let selectedId = $state<string | null>(null);
  let error = $state("");
  let loadFailed = $state(false);
  let refreshing = $state(false);
  let saving = $state<string[]>([]);
  let startingSetup = $state<string | null>(null);
  let startingInstall = $state<string | null>(null);
  let cancellingInstall = $state(false);
  let setupOperation = $state<AgentClientSetupOperation | null>(null);
  let installOperation = $state<AgentClientOperation | null>(null);
  let setupConnectionError = $state("");
  let installConnectionError = $state("");
  let setupLoading = $state(true);
  let installLoading = $state(true);
  let loading = $state(true);
  let streamController: AbortController | null = null;
  let settledInstallId: string | null = null;

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
  const installBusy = $derived(
    startingInstall !== null || installOperation?.status === "running" || cancellingInstall,
  );
  const operationBusy = $derived(setupBusy || installBusy);

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

  function installOperationFor(client: AgentClientView): AgentClientOperation | null {
    return installOperation?.client_type === client.client_type &&
      installOperation.client_key === client.client_key
      ? installOperation
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

  async function loadLatestInstall() {
    installOperation = await getLatestAgentClientOperation();
    installConnectionError = "";
    if (!installOperation) return;
    if (installOperation.output_offset > 0)
      installOperation = await loadFullOutput(installOperation);
    if (installOperation.status === "running") startInstallStream(installOperation);
  }

  async function loadFullOutput(operation: AgentClientOperation): Promise<AgentClientOperation> {
    let offset = 0;
    let output = "";
    let snapshot = operation;
    while (offset < operation.log_size) {
      snapshot = await getAgentClientOperation(operation.id, offset);
      if (snapshot.next_offset <= offset) break;
      output += snapshot.output;
      offset = snapshot.next_offset;
    }
    return { ...snapshot, output, output_offset: 0 };
  }

  async function refresh() {
    refreshing = true;
    error = "";
    try {
      await Promise.all([load(), loadLatestInstall()]);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      loadFailed = clients.length === 0;
    } finally {
      refreshing = false;
    }
  }

  function mergeInstallState(state: AgentClientOperationState, logSize: number) {
    if (!installOperation || installOperation.id !== state.id) return;
    installOperation = { ...installOperation, ...state, log_size: logSize };
  }

  async function settleInstall(previousStatus: AgentClientOperation["status"]) {
    if (!installOperation || installOperation.status === "running") return;
    installConnectionError = "";
    await load();
    if (
      previousStatus === "running" &&
      installOperation.status === "succeeded" &&
      settledInstallId !== installOperation.id
    ) {
      settledInstallId = installOperation.id;
      const installed = clients.find(
        (client) =>
          client.client_type === installOperation?.client_type &&
          client.client_key === installOperation?.client_key,
      );
      toast.success(
        translate("clients.install.toast", { name: installed?.name ?? "Agent Client" }),
        {
          description: translate("clients.install.toastBody"),
        },
      );
    }
  }

  function startInstallStream(operation: AgentClientOperation) {
    streamController?.abort();
    const controller = new AbortController();
    streamController = controller;
    void (async () => {
      while (!controller.signal.aborted && installOperation?.id === operation.id) {
        try {
          const response = await streamAgentClientOperation(
            operation.id,
            installOperation.next_offset,
            controller.signal,
          );
          installConnectionError = "";
          await readOperationStream(response.body!, (event) => {
            if (controller.signal.aborted || installOperation?.id !== operation.id) return;
            if (event.event === "log") {
              if (event.offset === installOperation.next_offset) {
                installOperation = {
                  ...installOperation,
                  output: installOperation.output + event.chunk,
                  next_offset: event.next_offset,
                  log_size: Math.max(installOperation.log_size, event.next_offset),
                };
              }
              return;
            }
            const previousStatus = installOperation.status;
            mergeInstallState(event.operation, event.log_size);
            if (event.event === "done") void settleInstall(previousStatus);
          });
          if (installOperation?.status !== "running") return;
          installConnectionError = translate("clients.install.reconnecting");
        } catch (cause) {
          if (controller.signal.aborted) return;
          installConnectionError = cause instanceof Error ? cause.message : String(cause);
          try {
            const snapshot = await getAgentClientOperation(
              operation.id,
              installOperation?.next_offset ?? 0,
              { signal: controller.signal },
            );
            if (installOperation?.id !== operation.id) return;
            const previousStatus = installOperation.status;
            installOperation = {
              ...snapshot,
              output: installOperation.output + snapshot.output,
            };
            if (snapshot.status !== "running") {
              await settleInstall(previousStatus);
              return;
            }
          } catch {
            // The device can be temporarily unavailable while Pontia integration restarts it.
          }
        }
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
    })();
  }

  onMount(() => {
    Promise.all([load(), loadLatestInstall()])
      .catch((cause) => {
        error = cause instanceof Error ? cause.message : String(cause);
        loadFailed = clients.length === 0;
      })
      .finally(() => {
        loading = false;
        installLoading = false;
      });
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
      streamController?.abort();
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

  async function install(client: AgentClientView, method: AgentClientInstallMethod) {
    startingInstall = client.id;
    error = "";
    try {
      installOperation = await installAgentClient(client, method.id);
      installConnectionError = "";
      if (installOperation.status === "running") startInstallStream(installOperation);
      else await settleInstall("running");
    } catch (cause) {
      if (cause instanceof ApiError && cause.status === 409) await loadLatestInstall();
      else throw cause;
    } finally {
      startingInstall = null;
    }
  }

  async function cancelInstall() {
    if (!installOperation || installOperation.status !== "running") return;
    cancellingInstall = true;
    error = "";
    try {
      const cancelled = await cancelAgentClientOperation(installOperation.id);
      installOperation = {
        ...installOperation,
        phase: cancelled.phase,
        status: cancelled.status,
        cancel_requested: cancelled.cancel_requested,
        error: cancelled.error,
        log_size: Math.max(installOperation.log_size, cancelled.log_size),
      };
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      cancellingInstall = false;
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
      if (!isMobileShell())
        toast.success(
          saved.enabled ? translate("clients.enabled") : translate("clients.disabled"),
          {
            description: translate("clients.sessionsUnchanged"),
          },
        );
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
    get installBusy() {
      return installBusy;
    },
    get activeInstall() {
      return installOperation;
    },
    get operationBusy() {
      return operationBusy;
    },
    get cancellingInstall() {
      return cancellingInstall;
    },
    get setupConnectionError() {
      return setupConnectionError;
    },
    get installConnectionError() {
      return installConnectionError;
    },
    get setupLoading() {
      return setupLoading;
    },
    get installLoading() {
      return installLoading;
    },
    get loading() {
      return loading;
    },
    saving: (client: AgentClientView) => saving.includes(client.id),
    operationFor,
    installOperationFor,
    profileCount,
    select,
    refresh,
    setup,
    install,
    cancelInstall,
    toggle,
    reset,
  };
}
