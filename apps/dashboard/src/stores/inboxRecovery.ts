import { writable } from "svelte/store";
import type { InboxMessageView, SubmitInboxMessageInput } from "../api/types";

export interface UnconfirmedSubmission {
  messageId: string;
  sessionId: string;
  input: SubmitInboxMessageInput;
  retryOf?: string;
  allowUnknown?: boolean;
}

const storageKey = "pontia:inbox-submissions:v1";

function readSaved(): UnconfirmedSubmission[] {
  if (typeof localStorage === "undefined") return [];
  const saved = localStorage.getItem(storageKey);
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
    throw new Error(
      "Saved Inbox submissions could not be read. Preserve browser storage to recover them.",
    );
  }
  return items;
}

export const unconfirmedSubmissions =
  writable<UnconfirmedSubmission[]>(readSaved());

function save(items: UnconfirmedSubmission[]): void {
  localStorage.setItem(storageKey, JSON.stringify(items));
}

function hide(isHidden: (item: UnconfirmedSubmission) => boolean): void {
  unconfirmedSubmissions.update((items) =>
    items.some(isHidden) ? items.filter((item) => !isHidden(item)) : items,
  );
}

export function rememberSubmission(submission: UnconfirmedSubmission): void {
  // Persist before sending: a reload must not turn an uncertain operation into a new input.
  // The submission is shown for recovery only once its receipt is actually unknown.
  save([
    ...readSaved().filter((item) => item.messageId !== submission.messageId),
    submission,
  ]);
}

export function exposeSubmission(submission: UnconfirmedSubmission): void {
  unconfirmedSubmissions.update((items) => [
    ...items.filter((item) => item.messageId !== submission.messageId),
    submission,
  ]);
}

export function forgetSubmission(messageId: string): void {
  save(readSaved().filter((item) => item.messageId !== messageId));
  hide((item) => item.messageId === messageId);
}

export function reconcileSubmissions(messages: InboxMessageView[]): void {
  const accepted = new Set(messages.map((message) => message.message_id));
  const saved = readSaved();
  if (saved.some((item) => accepted.has(item.messageId))) {
    save(saved.filter((item) => !accepted.has(item.messageId)));
  }
  hide((item) => accepted.has(item.messageId));
}

export class SubmissionUnconfirmedError extends Error {
  constructor() {
    super(
      "Submission receipt unknown. Use Check / recover submission to resume this same input.",
    );
  }
}
