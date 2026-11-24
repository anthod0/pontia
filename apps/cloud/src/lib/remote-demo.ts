// The homepage remote-control demo: one scripted Session that the terminal and the phone both render.

export type Prompt = "fix" | "auth" | "pr";
export type Task = "read" | "edit" | "tests-all" | "tests-auth" | "pr";
export type Reply = "cause" | "instead" | "passed" | "shipped";

export type Entry =
  | { kind: "prompt"; id: Prompt }
  | { kind: "step"; id: Task; done: boolean }
  | { kind: "ask"; answer: "approve" | "deny" | null }
  | { kind: "reply"; id: Reply };

/** What the Session waits for: an approval, a message, the agent's running step, or nothing. */
export type Waiting =
  | { kind: "approval" }
  | { kind: "message"; suggestion: "auth" | "pr" }
  | { kind: "agent" }
  | { kind: "finished" };

export type RemoteDemo = { entries: Entry[]; waiting: Waiting };

export function start(): RemoteDemo {
  return {
    entries: [
      { kind: "prompt", id: "fix" },
      { kind: "step", id: "read", done: true },
      { kind: "step", id: "edit", done: true },
      { kind: "reply", id: "cause" },
      { kind: "ask", answer: null },
    ],
    waiting: { kind: "approval" },
  };
}

function answer(demo: RemoteDemo, choice: "approve" | "deny"): Entry[] {
  return demo.entries.map((entry) =>
    entry.kind === "ask" && entry.answer === null ? { ...entry, answer: choice } : entry,
  );
}

export function approve(demo: RemoteDemo): RemoteDemo {
  return {
    entries: [...answer(demo, "approve"), { kind: "step", id: "tests-all", done: false }],
    waiting: { kind: "agent" },
  };
}

export function deny(demo: RemoteDemo): RemoteDemo {
  return {
    entries: [...answer(demo, "deny"), { kind: "reply", id: "instead" }],
    waiting: { kind: "message", suggestion: "auth" },
  };
}

/** Sends the suggested message and starts the step it asks for. */
export function send(demo: RemoteDemo, prompt: "auth" | "pr"): RemoteDemo {
  const task: Task = prompt === "auth" ? "tests-auth" : "pr";
  return {
    entries: [
      ...demo.entries,
      { kind: "prompt", id: prompt },
      { kind: "step", id: task, done: false },
    ],
    waiting: { kind: "agent" },
  };
}

/** Completes the running step and adds the agent's reply. */
export function finish(demo: RemoteDemo): RemoteDemo {
  const entries = demo.entries.map((entry) =>
    entry.kind === "step" && !entry.done ? { ...entry, done: true } : entry,
  );
  const shipped = entries.some((entry) => entry.kind === "step" && entry.id === "pr");
  return shipped
    ? { entries: [...entries, { kind: "reply", id: "shipped" }], waiting: { kind: "finished" } }
    : {
        entries: [...entries, { kind: "reply", id: "passed" }],
        waiting: { kind: "message", suggestion: "pr" },
      };
}
