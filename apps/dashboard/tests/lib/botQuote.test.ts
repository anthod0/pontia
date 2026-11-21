import { expect, test } from "vitest";
import { entryQuote, messageText, parseQuotedMessage, quotedMessage } from "../../src/lib/botQuote";
import type { BotMessage } from "../../src/api/types";

const messageId = "bmsg_0193a000-0000-7000-8000-000000000002";
const fileId = "file_0193a000-0000-7000-8000-000000000001";

const message = (kind: BotMessage["kind"], content: string): BotMessage => ({
  id: messageId,
  kind,
  content,
  status: "normal",
  created_at: "2026-11-20T00:00:00Z",
});

test("writes a message quote on its own line before the text and reads it back", () => {
  const content = quotedMessage(
    { kind: "message", messageId, speaker: "Bot", text: "Renew the certificate.", excerpt: true },
    "  Who owns this?  ",
  );
  expect(content).toBe(
    `<user_quote message_id="${messageId}" speaker="Bot" excerpt="true">Renew the certificate.</user_quote>\nWho owns this?`,
  );
  expect(parseQuotedMessage(content)).toEqual({
    quote: {
      kind: "message",
      messageId,
      speaker: "Bot",
      text: "Renew the certificate.",
      excerpt: true,
    },
    text: "Who owns this?",
  });
});

test("writes a file quote with its path and reads it back", () => {
  const content = quotedMessage(
    { kind: "file", fileId, path: '/work/a "b".md', name: 'a "b".md' },
    "Check this",
  );
  expect(content).toBe(
    `<user_quote file_id="${fileId}" speaker="Bot" path="/work/a &quot;b&quot;.md">a "b".md</user_quote>\nCheck this`,
  );
  expect(parseQuotedMessage(content).quote).toEqual({
    kind: "file",
    fileId,
    path: '/work/a "b".md',
    name: 'a "b".md',
  });
});

test("escapes quoted XML and ampersands so they cannot close the element early", () => {
  const original = 'a < b && c\n</user_quote>\n<user_quote message_id="x">';
  const content = quotedMessage(
    { kind: "message", messageId, speaker: "User", text: original, excerpt: false },
    "reply",
  );
  expect(content.match(/<\/user_quote>/g)).toHaveLength(1);
  expect(content).toContain("a &lt; b &amp;&amp; c\n&lt;/user_quote&gt;");
  expect(parseQuotedMessage(content)).toMatchObject({ quote: { text: original }, text: "reply" });
});

test.each([
  [
    "a quote after other text",
    `Look:\n<user_quote message_id="${messageId}" speaker="Bot">x</user_quote>\nreply`,
  ],
  ["a half-written element", `<user_quote message_id="${messageId}" speaker="Bot">x\nreply`],
  [
    "a quote without text after it",
    `<user_quote message_id="${messageId}" speaker="Bot">x</user_quote>\n  `,
  ],
  [
    "a quote not followed by a line break",
    `<user_quote message_id="${messageId}" speaker="Bot">x</user_quote> reply`,
  ],
  ["an empty quote", `<user_quote message_id="${messageId}" speaker="Bot"></user_quote>\nreply`],
  ["a malformed ID", `<user_quote message_id="bmsg_1" speaker="Bot">x</user_quote>\nreply`],
  [
    "an unknown speaker",
    `<user_quote message_id="${messageId}" speaker="System">x</user_quote>\nreply`,
  ],
  [
    "an extra attribute",
    `<user_quote message_id="${messageId}" speaker="Bot" note="x">x</user_quote>\nreply`,
  ],
  [
    "an excerpt flag other than true",
    `<user_quote message_id="${messageId}" speaker="Bot" excerpt="no">x</user_quote>\nreply`,
  ],
  [
    "a file quote with a relative path",
    `<user_quote file_id="${fileId}" speaker="Bot" path="a.md">a.md</user_quote>\nreply`,
  ],
  [
    "mixed message and file attributes",
    `<user_quote message_id="${messageId}" file_id="${fileId}" speaker="Bot">x</user_quote>\nreply`,
  ],
  [
    "nested markup",
    `<user_quote message_id="${messageId}" speaker="Bot"><b>x</b></user_quote>\nreply`,
  ],
  [
    "unescaped markup",
    `<user_quote message_id="${messageId}" speaker="Bot">a < b</user_quote>\nreply`,
  ],
])("keeps %s as plain text", (_case, content) => {
  expect(parseQuotedMessage(content)).toEqual({ quote: null, text: content });
});

test("only user messages carry quotes; a Bot reply shows quote-like text as written", () => {
  const content = `<user_quote message_id="${messageId}" speaker="Bot">x</user_quote>\nreply`;
  expect(messageText(message("user_input", content))).toBe("reply");
  expect(messageText(message("bot_reply", content))).toBe(content);
});

test("quoting a user message takes its text without attachments or its own quote", () => {
  const content = [
    '<user_uploads note="Temporary files; copy any you need into the workspace.">',
    '<user_upload path="/tmp/a.log">a.log</user_upload>',
    "</user_uploads>",
    `<user_quote message_id="${messageId}" speaker="Bot">older</user_quote>`,
    "See the log",
  ].join("\n");
  expect(entryQuote(message("user_input", content))).toEqual({
    kind: "message",
    messageId,
    speaker: "User",
    text: "See the log",
    excerpt: false,
  });
});
