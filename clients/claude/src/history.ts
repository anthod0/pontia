import { getSessionInfo, getSessionMessages } from "@anthropic-ai/claude-agent-sdk";

// Read-only SDK helpers do not start an execution instance.
export async function readHistory(session: string, cwd: string) {
  const info = await getSessionInfo(session, { dir: cwd });
  if (!info || info.sessionId !== session) throw new Error("source_unavailable");
  const messages = await getSessionMessages(session, { dir: cwd });
  // A bound Pontia conversation always contains a real input. SDK helpers may
  // return [] for missing/unreadable sources; never report that as empty history.
  if (!messages.length) throw new Error("source_unavailable");
  if (messages.some((message) => message.session_id !== session))
    throw new Error("source_identity_mismatch");
  return messages;
}
