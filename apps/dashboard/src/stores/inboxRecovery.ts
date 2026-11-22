import { translate } from "$lib/i18n";
import { readStored, scopedKey, writeStored } from "$lib/storage";
import { writable } from "svelte/store";

import type { InboxMessageView, SubmitInboxMessageInput } from "../api/types";
import type { DeviceRuntime } from "../device/runtime";

export interface UnconfirmedSubmission {
  messageId: string;
  sessionId: string;
  input: SubmitInboxMessageInput;
  retryOf?: string;
  allowUnknown?: boolean;
}

export class SubmissionUnconfirmedError extends Error {
  constructor() {
    super(translate("inbox.unknownSubmission"));
  }
}
export function createInboxRecovery(device: DeviceRuntime) {
  const storageKey = scopedKey("pontia:inbox-submissions:v2", device.id);

  // Unreadable data throws instead of reading as empty, so a later save cannot drop submissions.
  function readSaved(): UnconfirmedSubmission[] {
    const saved = readStored(storageKey);
    if (!saved) return [];
    const items: unknown = JSON.parse(saved);
    if (
      !Array.isArray(items) ||
      items.some(
        (item) =>
          !item ||
          typeof item.messageId !== "string" ||
          typeof item.sessionId !== "string" ||
          typeof item.input?.input !== "string",
      )
    ) {
      throw new Error(translate("inbox.storageUnreadable"));
    }
    return items;
  }

  const unconfirmedSubmissions = writable<UnconfirmedSubmission[]>(readSaved());

  function save(items: UnconfirmedSubmission[]): void {
    // A failed write throws, so a submission that cannot be saved is not sent.
    writeStored(storageKey, JSON.stringify(items), { required: true });
  }

  function hide(isHidden: (item: UnconfirmedSubmission) => boolean): void {
    unconfirmedSubmissions.update((items) =>
      items.some(isHidden) ? items.filter((item) => !isHidden(item)) : items,
    );
  }

  function rememberSubmission(submission: UnconfirmedSubmission): void {
    // Persist before sending: a reload must not turn an uncertain operation into a new input.
    // The submission is shown for recovery only once its receipt is actually unknown.
    save([...readSaved().filter((item) => item.messageId !== submission.messageId), submission]);
  }

  function exposeSubmission(submission: UnconfirmedSubmission): void {
    unconfirmedSubmissions.update((items) => [
      ...items.filter((item) => item.messageId !== submission.messageId),
      submission,
    ]);
  }

  function forgetSubmission(messageId: string): void {
    save(readSaved().filter((item) => item.messageId !== messageId));
    hide((item) => item.messageId === messageId);
  }

  function reconcileSubmissions(messages: InboxMessageView[]): void {
    const accepted = new Set(messages.map((message) => message.message_id));
    const saved = readSaved();
    if (saved.some((item) => accepted.has(item.messageId))) {
      save(saved.filter((item) => !accepted.has(item.messageId)));
    }
    hide((item) => accepted.has(item.messageId));
  }
  return {
    unconfirmedSubmissions,
    rememberSubmission,
    exposeSubmission,
    forgetSubmission,
    reconcileSubmissions,
  };
}
