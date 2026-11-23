import { createHash } from "node:crypto";
import type { CanUseTool, OnElicitation } from "@anthropic-ai/claude-agent-sdk";
import type { Emit, InteractionOption } from "./facts.js";
import { approval, questions, elicitation, type Card } from "./choices.js";

type Answers = string[] | "decline" | undefined;

// Object member order is immaterial. Array order, tool arguments and SDK permission
// context are part of the authorization. The signal and correlation ID are transport state.
function digest(tool: string, input: Record<string, unknown>, context: object): string {
  const content = JSON.stringify({ tool, input, context }, (_key, value: unknown) => {
    if (typeof value !== "object" || value === null || Array.isArray(value)) return value;
    return Object.fromEntries(
      Object.entries(value).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    );
  });
  return createHash("sha256").update(content).digest("hex");
}

function shorten(value: string, limit: number) {
  const characters = Array.from(value);
  return characters.length <= limit ? value : characters.slice(0, limit - 1).join("") + "…";
}

export class Interactions {
  private readonly pending = new Map<
    string,
    { options: InteractionOption[]; expire(): void; finish(key?: string): void }
  >();
  private readonly groups = new Map<
    string,
    { digest: string; answer: Promise<Answers>; expire(): void }
  >();

  constructor(private readonly emit: Emit) {}

  private wait(id: string, digest: string, signal: AbortSignal, blocking: boolean, cards: Card[]) {
    if (signal.aborted) return Promise.resolve(undefined);
    const reports = cards.map((card) => {
      const key = card.index === undefined ? id : id + ":" + card.index;
      return {
        kind: "interaction_requested" as const,
        interaction_key: key,
        blocking,
        authorization_digest:
          key === id
            ? digest
            : createHash("sha256")
                .update(digest + ":" + key)
                .digest("hex"),
        title: shorten(card.title, 256),
        description: shorten(card.description, 4000),
        options: card.options.map((o) => ({ ...o, label: shorten(o.label, 200) })),
      };
    });
    const existing = this.groups.get(id);
    if (existing) {
      if (existing.digest !== digest) {
        for (const report of reports)
          if (this.pending.has(report.interaction_key)) this.emit(report);
        existing.expire();
      }
      return existing.answer;
    }
    const answers: string[] = [];
    const keys = new Set<string>();
    let complete!: (answers: Answers) => void;
    const answer = new Promise<Answers>((resolve) => {
      complete = resolve;
    });
    const expire = () => finishGroup(undefined);
    const finishGroup = (result: Answers) => {
      for (const key of keys) this.pending.get(key)!.expire();
      signal.removeEventListener("abort", expire);
      this.groups.delete(id);
      complete(result);
    };
    this.groups.set(id, { digest, answer, expire });
    signal.addEventListener("abort", expire, { once: true });
    for (const [index, report] of reports.entries()) {
      const key = report.interaction_key;
      keys.add(key);
      const settle = (outcome: "resolved" | "expired") => {
        keys.delete(key);
        this.pending.delete(key);
        this.emit({
          kind: "interaction_settled",
          interaction_key: key,
          authorization_digest: report.authorization_digest,
          outcome,
        });
      };
      this.pending.set(key, {
        options: report.options,
        expire: () => settle("expired"),
        finish: (option) => {
          if (option === undefined) return expire();
          settle("resolved");
          if (option === "decline") return finishGroup(option);
          answers[index] = option;
          if (!keys.size) finishGroup(answers);
        },
      });
      this.emit(report);
    }
    return answer;
  }

  request: CanUseTool = async (tool, input, { signal, requestId, ...context }) => {
    const hash = digest(tool, input, context);
    if (tool === "AskUserQuestion") {
      const parsed = questions(input);
      if (typeof parsed === "string") return { behavior: "deny", message: parsed };
      const picked = await this.wait(requestId, hash, signal, !context.agentID, parsed.cards);
      if (!Array.isArray(picked))
        return {
          behavior: "deny",
          message:
            picked === "decline"
              ? "The user chose not to pick an option and will reply in chat."
              : "Question expired.",
        };
      return {
        behavior: "allow",
        updatedInput: { questions: input.questions, answers: parsed.answers(picked) },
      };
    }
    const { card, results } = approval(tool, input, context);
    const picked = await this.wait(requestId, hash, signal, !context.agentID, [card]);
    return Array.isArray(picked)
      ? results.get(picked[0]!)!
      : { behavior: "deny", message: "Tool approval expired." };
  };

  elicit: OnElicitation = async (request, { signal, requestId }) => {
    const parsed = elicitation(request);
    if (typeof parsed === "string") {
      console.error("Claude MCP elicitation declined:", parsed);
      return { action: "decline" };
    }
    const picked = await this.wait(
      "elicitation:" + requestId,
      digest("elicitation", request, {}),
      signal,
      true,
      parsed.cards,
    );
    if (!Array.isArray(picked)) return { action: picked === "decline" ? "decline" : "cancel" };
    return { action: "accept", content: parsed.answers(picked) };
  };

  resolve(id: string, option: string): boolean {
    const pending = this.pending.get(id);
    if (!pending?.options.some((o) => o.key === option)) return false;
    pending.finish(option);
    return true;
  }

  has(id: string): boolean {
    return this.pending.has(id);
  }
  cancel(id: string): void {
    this.pending.get(id)?.finish();
  }
  expire(): void {
    for (const group of this.groups.values()) group.expire();
  }
}
