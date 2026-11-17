export type QuestionReply = { question: string; answer: string };

const questionReplyBlock =
  /^\s*<send_user_message_question_reply>\s*([\s\S]*?)\s*<\/send_user_message_question_reply>\s*$/;

/** Parses the Codex answer envelope for asynchronous agent questions. */
export function parseQuestionReplies(content: string): QuestionReply[] | null {
  const body = content.match(questionReplyBlock)?.[1];
  if (!body) return null;
  let value: unknown;
  try {
    value = JSON.parse(body);
  } catch {
    return null;
  }
  if (!Array.isArray(value) || !value.length) return null;
  const replies = value.map((entry) =>
    entry && typeof entry.question === "string" && typeof entry.answer === "string"
      ? { question: entry.question, answer: entry.answer }
      : null,
  );
  return replies.every(Boolean) ? (replies as QuestionReply[]) : null;
}

export function questionRepliesText(replies: QuestionReply[]): string {
  return replies.map(({ question, answer }) => `> ${question}\n\n${answer}`).join("\n\n");
}
