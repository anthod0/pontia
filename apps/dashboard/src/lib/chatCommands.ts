interface ChatCommandInfo {
  description: string;
  disabledReason?: string;
  /** Whether a toggle command's state is currently on, such as /pin on a pinned session. */
  active?: boolean;
}

export type ChatCommand = ChatCommandInfo &
  (
    | {
        name:
          | "/new"
          | "/exit"
          | "/compact"
          | "/reload"
          | "/pin"
          | "/archive"
          | `/reasoning ${string}`;
        run: () => void;
      }
    | { name: "/rename"; run: (title: string) => void }
  );

export function findChatCommand(value: string, commands: ChatCommand[]): ChatCommand | undefined {
  if (/[\r\n]/.test(value)) return undefined;
  const input = value.trim();
  if (/^\/rename(?:[\t ]|$)/.test(input))
    return commands.find((command) => command.name === "/rename");
  return commands.find((command) => command.name === input);
}
