import { m } from "$lib/paraglide/messages";

// Avatar shapes and colours mirror apps/dashboard/src/components/bots/avatar.ts.
export const shapes = {
  mochi: "M51 30C72 30 84 44 88 62C91 74 92 84 80 84H22C10 84 11 74 14 62C18 44 30 30 51 30Z",
  drop: "M51 18C60 32 87 42 87 64C87 81 70 84 51 84C32 84 15 81 15 64C15 42 42 32 51 18Z",
  ghost: "M18 50C18 32 32 24 51 24C70 24 84 32 84 50V76Q73 90 62 79Q51 90 40 79Q29 90 18 76Z",
  blob: "M24 34C34 22 52 30 62 26C76 20 92 34 86 52C82 64 92 76 78 82C64 88 50 78 36 84C20 90 10 76 14 62C17 52 15 44 24 34Z",
  egg: "M51 18C70 18 84 44 84 62C84 78 70 86 51 86C32 86 18 78 18 62C18 44 32 18 51 18Z",
  pudding:
    "M30 28H72C78 28 80 31 81 36L88 76C89 82 86 84 80 84H22C16 84 13 82 14 76L21 36C22 31 24 28 30 28Z",
};
export type Shape = keyof typeof shapes;

type Text = () => string;
export type SessionState = "hidden" | "running" | "done";
type Line = {
  kind: "user" | "bot";
  text: Text;
  related?: Text;
  quote?: Text;
  todo?: boolean;
};

export type Agent = { client: string; logo: string; darkLogo?: string };

export const codex: Agent = {
  client: "Codex",
  logo: "/client-logos/codex.svg",
  darkLogo: "/client-logos/codex-dark.svg",
};
export const claude: Agent = { client: "Claude Code", logo: "/client-logos/claude-code.svg" };
export const pi: Agent = { client: "Pi", logo: "/client-logos/pi.svg" };

/** A scripted chat. Step i shows lines[0..i]; per-step arrays have one entry per line. */
export type Chat = {
  lines: Line[];
  sessions: (Agent & { title: Text; states: SessionState[] })[];
  status: (Text | null)[];
  todos: number[];
  /** Step at which the open todo is closed by the Bot, if any. */
  todoClosedAt?: number;
};

export type Visual = "sessions" | "models" | "local" | "memory" | "devices";

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
  {
    name: "Juno",
    shape: "drop",
    color: "#458588",
    label: m.story_work_label,
    caption: m.story_work_caption,
    visual: "sessions",
    steps: 5,
  },
  {
    name: "Ivy",
    shape: "ghost",
    color: "#98971a",
    label: m.story_local_label,
    caption: m.story_local_caption,
    visual: "local",
    steps: 2,
  },
  {
    name: "Kai",
    shape: "mochi",
    color: "#689d6a",
    label: m.story_models_label,
    caption: m.story_models_caption,
    visual: "models",
    steps: 3,
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
    name: "Tova",
    shape: "pudding",
    color: "#9c6b3c",
    label: m.story_devices_label,
    caption: m.story_devices_caption,
    visual: "devices",
    steps: 3,
  },
];

export const storyByName = (name: string) => stories.find((story) => story.name === name)!;
