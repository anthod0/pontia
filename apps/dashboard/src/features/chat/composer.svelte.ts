import { onDestroy } from "svelte";
import { toast } from "svelte-sonner";
import { translate } from "$lib/i18n";
import { uploadMessage } from "$lib/uploads/message";
import type { UploadDraftState } from "$lib/uploads/draft";
import { findChatCommand, type ChatCommand } from "$lib/chatCommands";
import type { InboxDeliveryPolicy } from "../../api/types";
import { useDeviceRuntime } from "../../device/context";

export function createComposer(options: {
  value: string;
  draftId?: string;
  retainDraft?: boolean;
  disabled?: boolean;
  busy?: boolean;
  submitDisabled?: boolean;
  commands: ChatCommand[];
  onSubmit: (message: string, policy: InboxDeliveryPolicy) => void | Promise<boolean | void>;
  closeExpanded: () => void;
}) {
  const device = useDeviceRuntime();
  const { createUploadDraft } = device.uploadDrafts;
  let attachments = $state<UploadDraftState>({ ready: false, items: [], warning: null });
  let uploads = $state<ReturnType<typeof createUploadDraft> | null>(null);
  $effect(() => {
    if (!options.draftId) return;
    const retained = options.retainDraft;
    const draft = retained
      ? device.uploadDrafts.getRetainedUploadDraft(options.draftId)
      : createUploadDraft(options.draftId);
    uploads = draft;
    const unsubscribe = draft.subscribe((state) => {
      attachments = state;
    });
    return () => {
      unsubscribe();
      if (!retained) draft.destroy();
    };
  });
  let sentAwaitingCleanup = $state(false);
  let sending = $state(false);
  const submissionToastId = "composer-submit-error";
  onDestroy(() => toast.dismiss(submissionToastId));
  const uploadBlocked = $derived(
    Boolean(options.draftId) &&
      (!attachments.ready || attachments.items.some((item) => item.status !== "complete")),
  );
  const completedFiles = $derived(
    attachments.items.flatMap((item) =>
      !item.secret && item.status === "complete" && item.upload?.path
        ? [{ name: item.name, path: item.upload.path }]
        : [],
    ),
  );
  const completedSecrets = $derived(
    attachments.items.flatMap((item) =>
      item.secret && item.status === "complete" && item.upload?.path
        ? [{ ...item.secret, path: item.upload.path }]
        : [],
    ),
  );
  const uploadUnavailable = $derived(
    options.disabled || options.busy || sending || !attachments.ready,
  );
  const command = $derived(findChatCommand(options.value, options.commands));
  const commandArgument = $derived(
    command ? options.value.trim().slice(command.name.length).trim() : "",
  );
  const cannotSubmit = $derived(
    options.disabled ||
      options.busy ||
      sending ||
      (command
        ? Boolean(command.disabledReason) || (command.name === "/rename" && !commandArgument)
        : options.submitDisabled ||
          uploadBlocked ||
          (!options.value.trim() && completedFiles.length === 0 && completedSecrets.length === 0)),
  );

  /** Returns false when the composer cannot submit right now. */
  function submit(deliveryPolicy: InboxDeliveryPolicy = "steer"): boolean {
    if (cannotSubmit) return false;
    if (command) executeCommand(command);
    else void submitMessage(deliveryPolicy);
    return true;
  }

  async function submitMessage(deliveryPolicy: InboxDeliveryPolicy): Promise<void> {
    const draft = uploads;
    sending = true;
    toast.dismiss(submissionToastId);
    try {
      if (!sentAwaitingCleanup)
        sentAwaitingCleanup =
          (await options.onSubmit(
            uploadMessage(options.value, completedFiles, completedSecrets),
            deliveryPolicy,
          )) === true;
      if (sentAwaitingCleanup) {
        await draft?.clear();
        sentAwaitingCleanup = false;
      }
    } catch (error) {
      // Sent messages keep their attachments until cleanup succeeds, so the error stays until the user retries.
      toast.error(error instanceof Error ? error.message : String(error), {
        id: submissionToastId,
        duration: Number.POSITIVE_INFINITY,
        important: true,
        action: {
          label: translate("common.retry"),
          onClick: (event) => {
            if (!submit(deliveryPolicy)) event.preventDefault();
          },
        },
      });
    } finally {
      sending = false;
    }
  }

  function executeCommand(command: ChatCommand): void {
    if (options.disabled || options.busy || command.disabledReason) return;
    if (command.name === "/rename") {
      if (!commandArgument) return;
      options.closeExpanded();
      command.run(commandArgument);
      return;
    }
    options.value = command.name;
    options.closeExpanded();
    command.run();
  }

  return {
    get attachments() {
      return attachments;
    },
    get uploads() {
      return uploads;
    },
    get sentAwaitingCleanup() {
      return sentAwaitingCleanup;
    },
    get sending() {
      return sending;
    },
    get uploadUnavailable() {
      return uploadUnavailable;
    },
    get cannotSubmit() {
      return cannotSubmit;
    },
    get command() {
      return command;
    },
    submit,
    executeCommand,
  };
}
export type ComposerController = ReturnType<typeof createComposer>;
