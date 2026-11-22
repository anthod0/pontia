import { m } from "$lib/paraglide/messages";

// Avatar shapes and colours mirror apps/dashboard/src/components/bots/avatar.ts.
export const shapes = {
  classic:
    "M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z",
  mochi: "M51 30C72 30 84 44 88 62C91 74 92 84 80 84H22C10 84 11 74 14 62C18 44 30 30 51 30Z",
  cloud:
    "M18 72C8 62 12 46 22 42C22 30 36 26 44 32C48 24 62 24 66 32C74 26 88 32 84 44C94 50 92 66 82 74C68 86 32 86 18 72Z",
  drop: "M51 18C60 32 87 42 87 64C87 81 70 84 51 84C32 84 15 81 15 64C15 42 42 32 51 18Z",
  blob: "M24 34C34 22 52 30 62 26C76 20 92 34 86 52C82 64 92 76 78 82C64 88 50 78 36 84C20 90 10 76 14 62C17 52 15 44 24 34Z",
  ghost: "M18 50C18 32 32 24 51 24C70 24 84 32 84 50V76Q73 90 62 79Q51 90 40 79Q29 90 18 76Z",
  onigiri: "M51.0 32.0Q68.6 50.3 79.0 72.0Q51.0 76.0 23.0 72.0Q33.4 50.3 51.0 32.0Z",
  egg: "M51 18C70 18 84 44 84 62C84 78 70 86 51 86C32 86 18 78 18 62C18 44 32 18 51 18Z",
  pudding:
    "M30 28H72C78 28 80 31 81 36L88 76C89 82 86 84 80 84H22C16 84 13 82 14 76L21 36C22 31 24 28 30 28Z",
};
export type Shape = keyof typeof shapes;

type Text = () => string;
export type SessionState = "hidden" | "running" | "done" | "error";
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

export type Agent = { client: string; logo?: string; darkLogo?: string };

export const codex: Agent = {
  client: "Codex",
  logo: "/client-logos/codex.svg",
  darkLogo: "/client-logos/codex-dark.svg",
};
export const claude: Agent = { client: "Claude Code", logo: "/client-logos/claude-code.svg" };
export const pi: Agent = { client: "Pi", logo: "/client-logos/pi.svg" };
export const acp: Agent = { client: "ACP Agent" };

/** A scripted chat. Step i shows lines[0..i]; per-step arrays have one entry per line. */
export type Chat = {
  lines: Line[];
  sessions: (Agent & { title: Text; states: SessionState[] })[];
  status: (Text | null)[];
  todos: number[];
  /** Step at which the open todo is closed by the Bot, if any. */
  todoClosedAt?: number;
};

export type Visual = "profiles" | "worktrees" | "memory" | "files" | "devices" | "restart";

/** One Bot tab. Its demo plays steps 0..steps-1 and the last step is the complete picture. */
export type Story = {
  name: string;
  shape: Shape;
  color: string;
  label: Text;
  caption: Text;
  steps: number;
} & ({ chat: Chat; visual?: never } | { visual: Visual; chat?: never });

function chatStory(story: Omit<Story, "steps" | "visual" | "chat"> & { chat: Chat }): Story {
  return { ...story, steps: story.chat.lines.length };
}

export const stories: Story[] = [
  chatStory({
    name: "Atlas",
    shape: "classic",
    color: "#458588",
    label: m.story_context_label,
    caption: m.story_context_caption,
    chat: {
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
  }),
  chatStory({
    name: "Mika",
    shape: "egg",
    color: "#d79921",
    label: m.story_todo_label,
    caption: m.story_todo_caption,
    chat: {
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
        {
          ...claude,
          title: m.story_task_api,
          states: ["hidden", "running", "done", "done", "done"],
        },
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
  }),
  chatStory({
    name: "Juno",
    shape: "cloud",
    color: "#5f6fa3",
    label: m.story_alert_label,
    caption: m.story_alert_caption,
    chat: {
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
  }),
  {
    name: "Kai",
    shape: "mochi",
    color: "#689d6a",
    label: m.story_profiles_label,
    caption: m.story_profiles_caption,
    visual: "profiles",
    steps: 5,
  },
  {
    name: "Ivy",
    shape: "drop",
    color: "#98971a",
    label: m.story_worktrees_label,
    caption: m.story_worktrees_caption,
    visual: "worktrees",
    steps: 5,
  },
  {
    name: "Remy",
    shape: "blob",
    color: "#b16286",
    label: m.story_memory_label,
    caption: m.story_memory_caption,
    visual: "memory",
    steps: 4,
  },
  {
    name: "Pip",
    shape: "onigiri",
    color: "#d65d0e",
    label: m.story_files_label,
    caption: m.story_files_caption,
    visual: "files",
    steps: 4,
  },
  {
    name: "Tova",
    shape: "pudding",
    color: "#9c6b3c",
    label: m.story_devices_label,
    caption: m.story_devices_caption,
    visual: "devices",
    steps: 3,
  },
  {
    name: "Sol",
    shape: "ghost",
    color: "#c45a76",
    label: m.story_restart_label,
    caption: m.story_restart_caption,
    visual: "restart",
    steps: 4,
  },
];

export const storyByName = (name: string) => stories.find((story) => story.name === name)!;
