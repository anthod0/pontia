import { m } from "$lib/paraglide/messages";

// Avatar shapes and colours mirror apps/dashboard/src/components/bots/avatar.ts.
export const shapes = {
  classic:
    "M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z",
  cloud:
    "M18 72C8 62 12 46 22 42C22 30 36 26 44 32C48 24 62 24 66 32C74 26 88 32 84 44C94 50 92 66 82 74C68 86 32 86 18 72Z",
  egg: "M51 18C70 18 84 44 84 62C84 78 70 86 51 86C32 86 18 78 18 62C18 44 32 18 51 18Z",
};
export type Shape = keyof typeof shapes;

type Text = () => string;
type SessionState = "hidden" | "running" | "done" | "error";
type Line =
  | {
      kind: "user" | "bot";
      text: Text;
      earlier?: boolean;
      related?: Text;
      quote?: Text;
      todo?: boolean;
      alert?: boolean;
    }
  | { kind: "divider"; text: Text };

/** One scripted chat. Step i shows lines[0..i]; per-step arrays have one entry per line. */
export type Story = {
  name: string;
  shape: Shape;
  color: string;
  label: Text;
  caption: Text;
  lines: Line[];
  sessions: {
    client: string;
    logo: string;
    darkLogo?: string;
    title: Text;
    states: SessionState[];
  }[];
  status: (Text | null)[];
  todos: number[];
  /** Step at which the open todo is closed by the Bot, if any. */
  todoClosedAt?: number;
};

const codex = {
  client: "Codex",
  logo: "/client-logos/codex.svg",
  darkLogo: "/client-logos/codex-dark.svg",
};
const claude = { client: "Claude Code", logo: "/client-logos/claude-code.svg" };
const pi = { client: "Pi", logo: "/client-logos/pi.svg" };

export const stories: Story[] = [
  {
    name: "Atlas",
    shape: "classic",
    color: "#458588",
    label: m.story_context_label,
    caption: m.story_context_caption,
    lines: [
      { kind: "user", text: m.story_context_ask, earlier: true },
      { kind: "bot", text: m.story_context_reply, earlier: true },
      { kind: "divider", text: m.story_context_divider },
      { kind: "user", text: m.story_context_follow_up },
      { kind: "bot", text: m.story_context_answer, related: m.story_context_ask },
    ],
    sessions: [
      {
        ...pi,
        title: m.story_context_task,
        states: ["hidden", "hidden", "hidden", "running", "done"],
      },
    ],
    status: [null, null, null, m.story_context_task, null],
    todos: [0, 0, 0, 0, 0],
  },
  {
    name: "Mika",
    shape: "egg",
    color: "#d79921",
    label: m.story_todo_label,
    caption: m.story_todo_caption,
    lines: [
      { kind: "user", text: m.story_todo_ask },
      { kind: "bot", text: m.story_todo_plan },
      { kind: "bot", text: m.story_todo_question, todo: true },
      { kind: "user", text: m.story_todo_answer, quote: m.story_todo_question },
      { kind: "bot", text: m.story_todo_report, related: m.story_todo_question },
    ],
    sessions: [
      {
        ...codex,
        title: m.story_task_ui,
        states: ["hidden", "running", "running", "running", "done"],
      },
      { ...claude, title: m.story_task_api, states: ["hidden", "running", "done", "done", "done"] },
      {
        ...pi,
        title: m.story_task_tests,
        states: ["hidden", "running", "running", "done", "done"],
      },
    ],
    status: [null, m.story_task_ui, m.story_task_ui, m.story_task_ui, null],
    todos: [0, 0, 1, 1, 0],
    todoClosedAt: 4,
  },
  {
    name: "Juno",
    shape: "cloud",
    color: "#5f6fa3",
    label: m.story_alert_label,
    caption: m.story_alert_caption,
    lines: [
      { kind: "user", text: m.story_alert_ask },
      { kind: "bot", text: m.story_alert_ack },
      { kind: "bot", text: m.story_alert_failure, alert: true },
      { kind: "bot", text: m.story_alert_report, related: m.story_alert_ask },
    ],
    sessions: [
      { ...claude, title: m.story_task_api, states: ["hidden", "running", "error", "error"] },
      { ...claude, title: m.story_alert_retry, states: ["hidden", "hidden", "running", "done"] },
    ],
    status: [null, m.story_task_api, m.story_alert_retry, null],
    todos: [0, 0, 0, 0],
  },
];
