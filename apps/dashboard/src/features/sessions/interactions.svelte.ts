import { translate } from "$lib/i18n";
import { onMount, untrack } from "svelte";
import { fromStore } from "svelte/store";
import { toast } from "svelte-sonner";
import { isMobileShell } from "$lib/hooks/shellMode.svelte";
import { useDeviceRuntime } from "../../device/context";
import { ApiError } from "../../api/errors";
import type { Interaction } from "../../api/types";
export function createInteractions(sessionId: () => string) {
  const device = useDeviceRuntime();
  const { getInteraction, listPendingInteractions, respondInteraction } = device.api;
  const { subscribeDashboardEvents } = device.events;
  const { sseStatus } = device.connection;

  let interactions = $state.raw<Interaction[]>([]);
  let submitting = $state.raw(new Set<string>());
  let error = $state<string | null>(null);
  let lastToastedError: string | null = null;
  let mounted = $state(false);
  let generation = 0;
  let activeSession = "";
  let refreshRunning = false;
  let refreshQueued = false;

  async function refresh() {
    refreshQueued = true;
    if (refreshRunning) return;
    refreshRunning = true;
    try {
      while (refreshQueued && mounted) {
        refreshQueued = false;
        const current = generation;
        const session = sessionId();
        if (!session) continue;
        try {
          const snapshot = await listPendingInteractions(session);
          const resolving = interactions.filter((i) => i.state === "resolving");
          const updates = await Promise.all(
            resolving.map((i) => getInteraction(session, i.interaction_id)),
          );
          if (current === generation) {
            for (const update of updates) {
              if (
                interactions.find((i) => i.interaction_id === update.interaction_id)?.state ===
                "resolving"
              )
                recordResult(update);
            }
            const outcomes = interactions.filter((i) => i.state !== "pending");
            interactions = [
              ...snapshot.filter(
                (i) => !outcomes.some((o) => o.interaction_id === i.interaction_id),
              ),
              ...outcomes,
            ];
            error = null;
          }
        } catch (cause) {
          if (current === generation)
            error = cause instanceof Error ? cause.message : String(cause);
        }
      }
    } finally {
      refreshRunning = false;
    }
  }

  function recordResult(result: Interaction) {
    const index = interactions.findIndex((i) => i.interaction_id === result.interaction_id);
    interactions =
      index < 0
        ? [...interactions, result]
        : interactions.map((i) => (i.interaction_id === result.interaction_id ? result : i));
  }

  async function respond(interaction: Interaction, key: string) {
    if (submitting.has(interaction.interaction_id)) return;
    const current = generation;
    const session = sessionId();
    submitting = new Set(submitting).add(interaction.interaction_id);
    error = null;
    try {
      const result = await respondInteraction(session, interaction.interaction_id, {
        option_key: key,
      });
      if (current === generation) recordResult(result);
    } catch (cause) {
      if (current !== generation) return;
      if (cause instanceof ApiError && cause.status === 409) {
        try {
          const result = await getInteraction(session, interaction.interaction_id);
          if (current === generation) recordResult(result);
        } catch (readError) {
          if (current === generation)
            error = readError instanceof Error ? readError.message : String(readError);
        }
      } else error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (current === generation) {
        const next = new Set(submitting);
        next.delete(interaction.interaction_id);
        submitting = next;
      }
    }
  }

  $effect(() => {
    if (!isMobileShell() && error && error !== lastToastedError) {
      toast.error(translate("interaction.failed"), { description: error, duration: 6000 });
      lastToastedError = error;
    }
    if (!error) lastToastedError = null;
  });

  $effect(() => {
    const session = sessionId();
    if (!mounted || session === activeSession) return;
    untrack(() => {
      activeSession = session;
      generation += 1;
      interactions = [];
      submitting = new Set();
      error = null;
      void refresh();
    });
  });
  const connection = fromStore(sseStatus);
  $effect(() => {
    if (mounted && connection.current === "open") untrack(refresh);
  });

  onMount(() => {
    mounted = true;
    const unsubscribe = subscribeDashboardEvents((event) => {
      if (
        event.kind === "interactions_resync" ||
        (event.kind === "interaction_changed" && event.session_id === sessionId())
      )
        void refresh();
    });
    const visible = () => {
      if (document.visibilityState === "visible") void refresh();
    };
    document.addEventListener("visibilitychange", visible);
    window.addEventListener("pageshow", visible);
    return () => {
      mounted = false;
      generation += 1;
      unsubscribe();
      document.removeEventListener("visibilitychange", visible);
      window.removeEventListener("pageshow", visible);
    };
  });

  return {
    get interactions() {
      return interactions;
    },
    get submitting() {
      return submitting;
    },
    get error() {
      return error;
    },
    respond,
  };
}
