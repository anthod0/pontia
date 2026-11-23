import { expect, test } from "vitest";
import { searchSnippet } from "../../src/lib/botSearch";
import type { BotMessage } from "../../src/api/types";

const message = (kind: BotMessage["kind"], content: string): BotMessage => ({
  id: "bmsg_0193a000-0000-7000-8000-000000000002",
  kind,
  content,
  status: "normal",
  created_at: "2026-11-20T00:00:00Z",
});
const marked = (parts: ReturnType<typeof searchSnippet>) =>
  parts.map((part) => (part.match ? `[${part.text}]` : part.text)).join("");

test("marks every case-insensitive match on one line", () => {
  expect(marked(searchSnippet(message("bot_reply", "Deploy\n\nthen deploy again"), "DEPLOY"))).toBe(
    "[Deploy] then [deploy] again",
  );
});

test("starts the excerpt shortly before a late first match", () => {
  const text = `${"前".repeat(60)}部署完成`;
  expect(marked(searchSnippet(message("user_input", text), "部署"))).toBe(
    `…${"前".repeat(40)}[部署]完成`,
  );
});

test("shows the user's own text before the quote it replies to", () => {
  const content = `<user_quote message_id="bmsg_0193a000-0000-7000-8000-000000000001" speaker="Bot">部署计划</user_quote>\n部署吧`;
  expect(marked(searchSnippet(message("user_input", content), "部署"))).toBe("[部署]吧");
  expect(marked(searchSnippet(message("user_input", content), "计划"))).toBe("部署[计划]");
});

test("falls back to the stored text when only markup matches", () => {
  expect(marked(searchSnippet(message("bot_reply", "Use **bold**"), "**b"))).toBe("Use [**b]old**");
});

test("folds only ASCII letters, as the server does", () => {
  expect(marked(searchSnippet(message("bot_reply", "Café CAFÉ İstanbul"), "café"))).toBe(
    "[Café] CAFÉ İstanbul",
  );
  expect(marked(searchSnippet(message("bot_reply", "İstanbul and istanbul"), "stan"))).toBe(
    "İ[stan]bul and i[stan]bul",
  );
});

test("shows an uploaded file's path when only the path matches", () => {
  const content = [
    '<user_uploads note="Temporary files; copy any you need into the workspace.">',
    '<user_upload path="/uploads/报告/a.pdf">a.pdf</user_upload>',
    "</user_uploads>",
    "See attached",
  ].join("\n");
  expect(marked(searchSnippet(message("user_input", content), "报告"))).toBe(
    "/uploads/[报告]/a.pdf",
  );
});
