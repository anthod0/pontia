import { query, type Query } from "@anthropic-ai/claude-agent-sdk";
import { Input } from "./input.js";
import type { QueryFactory } from "./host.js";

export async function modelOptions(execution: Pick<Query, "supportedModels">) {
  return (await execution.supportedModels()).map((model) => ({
    id: model.value,
    name: model.displayName,
    description: model.description,
  }));
}

export async function listModels(cwd: string, createQuery: QueryFactory = query) {
  const input = new Input();
  const execution = createQuery({
    prompt: input,
    options: {
      cwd,
      persistSession: false,
      settingSources: ["user", "project", "local"],
      stderr: () => {},
    },
  });
  try {
    return await modelOptions(execution);
  } finally {
    input.end();
    execution.close();
  }
}
