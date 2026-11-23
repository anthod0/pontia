import type {
  CanUseTool,
  ElicitationRequest,
  PermissionResult,
  PermissionUpdate,
} from "@anthropic-ai/claude-agent-sdk";
import type { InteractionOption } from "./facts.js";

export type Card = {
  index?: number;
  title: string;
  description: string;
  options: InteractionOption[];
};

function scope(update: PermissionUpdate): string {
  switch (update.type) {
    case "setMode":
      return update.mode + " mode";
    case "addDirectories":
      return "access " + update.directories.join(", ");
    case "removeDirectories":
      return "remove access to " + update.directories.join(", ");
    default: {
      const rules = update.rules.map((rule) =>
        rule.ruleContent ? rule.toolName + "(" + rule.ruleContent + ")" : rule.toolName,
      );
      return (
        (update.type === "addRules" && update.behavior === "allow"
          ? ""
          : update.type + " " + update.behavior + ": ") + rules.join(", ")
      );
    }
  }
}

const positive = (key: string, label: string): InteractionOption => ({
  key,
  label,
  intent: "positive",
});
const negative = (key: string, label: string): InteractionOption => ({
  key,
  label,
  intent: "negative",
});

export function approval(
  tool: string,
  input: Record<string, unknown>,
  context: Omit<Parameters<CanUseTool>[2], "signal" | "requestId">,
) {
  const options = [positive("allow", "Allow once")];
  const results = new Map<string, PermissionResult>();
  results.set("allow", {
    behavior: "allow",
    updatedInput: input,
    decisionClassification: "user_temporary",
  });
  if (context.suggestions?.length && !context.suppressAlwaysAllowRule) {
    options.push(
      positive(
        "allow_always",
        "Always allow in this session: " + context.suggestions.map(scope).join("; "),
      ),
    );
    results.set("allow_always", {
      behavior: "allow",
      updatedInput: input,
      updatedPermissions: context.suggestions.map((update) => ({
        ...update,
        destination: "session",
      })),
      decisionClassification: "user_permanent",
    });
  }
  options.push(negative("deny", "Deny"));
  results.set("deny", {
    behavior: "deny",
    message: "User denied this tool use.",
    decisionClassification: "user_reject",
  });
  const summary =
    tool === "Bash"
      ? input.command
      : ["Read", "Edit", "Write"].includes(tool)
        ? input.file_path
        : JSON.stringify(input);
  return {
    results,
    card: {
      title: context.title ?? "Allow " + (context.displayName ?? tool) + "?",
      description: [context.description, context.decisionReason, context.blockedPath, summary]
        .filter(Boolean)
        .join("\n"),
      options,
    },
  };
}

function object(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

type Choice = { label: string; value: string | boolean };
type Field = { name: string; title: string; description: string; choices: Choice[] };
function fields(fields: Field[], decline: string) {
  const cards = fields.map((field, index) => ({
    index,
    title: field.title,
    description: field.description,
    options: [
      ...field.choices.slice(0, 7).map((choice, j) => positive("option_" + j, choice.label)),
      negative("decline", decline),
    ],
  }));
  return {
    cards,
    answers: (picked: string[]) =>
      Object.fromEntries(
        fields.map((field, i) => {
          const index = cards[i]!.options.findIndex((o) => o.key === picked[i]);
          return [field.name, field.choices[index]!.value];
        }),
      ),
  };
}

export function questions(input: Record<string, unknown>) {
  const questions = input.questions;
  const invalid = "Invalid question. Ask the user in plain text.";
  if (!Array.isArray(questions) || !questions.length) return invalid;
  const parsed: Field[] = [];
  for (const [index, q] of questions.entries()) {
    if (!object(q)) return invalid;
    if (q.multiSelect === true)
      return "Multi-select questions are not supported in this session. Ask single-choice questions, or ask the user in plain text.";
    if (
      typeof q.question !== "string" ||
      typeof q.header !== "string" ||
      !Array.isArray(q.options) ||
      !q.options.length
    )
      return invalid;
    const choices: Choice[] = [];
    const descriptions: string[] = [];
    for (const option of q.options) {
      if (
        !object(option) ||
        typeof option.label !== "string" ||
        typeof option.description !== "string"
      )
        return invalid;
      choices.push({ label: option.label, value: option.label });
      descriptions.push(option.label + " — " + option.description);
    }
    const prefix = questions.length > 1 ? "(" + (index + 1) + "/" + questions.length + ") " : "";
    parsed.push({
      name: q.question,
      title: prefix + q.header + ": " + q.question,
      description: descriptions.join("\n"),
      choices,
    });
  }
  return fields(parsed, "None of these, I'll reply in chat");
}

function fieldChoices(schema: Record<string, unknown>): Choice[] | undefined {
  if (schema.type === "boolean")
    return [
      { label: "Yes", value: true },
      { label: "No", value: false },
    ];
  if (schema.type !== "string") return;
  if (Array.isArray(schema.enum) && schema.enum.every((v) => typeof v === "string")) {
    const names = schema.enumNames;
    return schema.enum.map((value, i) => ({
      value,
      label: Array.isArray(names) && typeof names[i] === "string" ? names[i] : value,
    }));
  }
  if (Array.isArray(schema.oneOf)) {
    const choices: Choice[] = [];
    for (const variant of schema.oneOf) {
      if (!object(variant) || typeof variant.const !== "string") return;
      choices.push({
        value: variant.const,
        label: typeof variant.title === "string" ? variant.title : variant.const,
      });
    }
    return choices;
  }
}

export function elicitation(request: ElicitationRequest) {
  if (request.mode === "url")
    return "URL confirmation requires a browser. Ask the user in plain text.";
  const title = request.title ?? request.displayName ?? request.serverName;
  const schema = request.requestedSchema;
  const parsed: Field[] = [];
  for (const [name, field] of Object.entries(schema?.properties ?? {})) {
    const choices = object(field) && fieldChoices(field);
    if (!choices || !choices.length) {
      if (
        object(field) &&
        field.type === "string" &&
        !(Array.isArray(schema?.required) && schema.required.includes(name))
      )
        continue;
      return "This form requires an unsupported field. Ask the user in plain text.";
    }
    parsed.push({
      name,
      title,
      description: [request.message, field.title ?? name, field.description]
        .filter(Boolean)
        .join("\n"),
      choices,
    });
  }
  if (parsed.length) return fields(parsed, "Decline");
  return {
    cards: [
      {
        title,
        description: request.message,
        options: [positive("accept", "Accept"), negative("decline", "Decline")],
      },
    ],
    answers: () => ({}),
  };
}
