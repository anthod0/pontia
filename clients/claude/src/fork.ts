import {
  forkSession,
  getSessionMessages,
  type ForkSessionOptions,
  type ForkSessionResult,
  type SessionMessage,
} from "@anthropic-ai/claude-agent-sdk";
import { randomUUID } from "node:crypto";

type ForkSdk = {
  messages(session: string, options: { dir: string }): Promise<SessionMessage[]>;
  fork(session: string, options: ForkSessionOptions): Promise<ForkSessionResult>;
  uuid(): string;
};

const sdk: ForkSdk = {
  messages: getSessionMessages,
  fork: forkSession,
  uuid: randomUUID,
};

export type HistoryForkResult = {
  session_id: string;
  resume: boolean;
  message_ids: Record<string, string>;
};

export async function forkHistory(
  session: string,
  targetMessageId: string,
  requestedMessageIds: string[],
  cwd: string,
  client: ForkSdk = sdk,
): Promise<HistoryForkResult> {
  const source = await client.messages(session, { dir: cwd });
  if (!source.length || source.some((message) => message.session_id !== session))
    throw new Error("source_unavailable");
  const target = source.findIndex((message) => message.uuid === targetMessageId);
  if (target < 0 || source.at(target)?.type !== "user")
    throw new Error("fork_target_unavailable");
  if (target === 0) {
    return { session_id: client.uuid(), resume: false, message_ids: {} };
  }

  const copiedSource = source.slice(0, target);
  const result = await client.fork(session, {
    dir: cwd,
    upToMessageId: copiedSource.at(-1)!.uuid,
  });
  if (result.sessionId === session) throw new Error("fork_identity_unchanged");
  const copied = await client.messages(result.sessionId, { dir: cwd });
  if (
    copied.length !== copiedSource.length ||
    copied.some((message) => message.session_id !== result.sessionId)
  )
    throw new Error("fork_history_mismatch");
  const requested = new Set(requestedMessageIds);
  const messageIds = Object.fromEntries(
    copiedSource.flatMap((message, index) =>
      requested.has(message.uuid) ? [[message.uuid, copied.at(index)!.uuid]] : [],
    ),
  );
  return { session_id: result.sessionId, resume: true, message_ids: messageIds };
}
