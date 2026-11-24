import type { Entry, Prompt, Reply, Task } from "$lib/remote-demo";
import { m } from "$lib/paraglide/messages";

export const command = "bun test";

const commands: Record<Exclude<Task, "read" | "edit">, string> = {
  "tests-all": command,
  "tests-auth": "bun test auth",
  pr: "gh pr create",
};

export const prompts: Record<Prompt, () => string> = {
  fix: m.remote_demo_prompt_fix,
  auth: m.remote_demo_prompt_auth,
  pr: m.remote_demo_prompt_pr,
};

export const replies: Record<Reply, () => string> = {
  cause: m.remote_demo_reply_cause,
  instead: m.remote_demo_reply_instead,
  passed: m.remote_demo_reply_passed,
  shipped: m.remote_demo_reply_shipped,
};

export function stepText(entry: Extract<Entry, { kind: "step" }>): string {
  if (entry.id === "read") return m.remote_demo_step_read();
  if (entry.id === "edit") return m.remote_demo_step_edit({ file: "src/auth/login.ts" });
  const name = commands[entry.id];
  return entry.done
    ? m.remote_demo_step_ran({ command: name })
    : m.remote_demo_step_running({ command: name });
}
