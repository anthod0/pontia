interface ChatCommandInfo {
  description: string;
  disabledReason?: string;
  /** Whether a toggle command's state is currently on, such as /pin on a pinned session. */
  active?: boolean;
}

export type ChatCommand = ChatCommandInfo &
  (
    | { name: "/new" | "/exit" | "/compact" | "/reload" | "/pin" | "/archive"; run: () => void }
    | { name: "/rename"; run: (title: string) => void }
  );

export function findChatCommand(value: string, commands: ChatCommand[]): ChatCommand | undefined {
  if (/[\r\n]/.test(value)) return undefined;
  const match = value.match(/^[\t ]*(\/[a-z]+)(?:[\t ]+([^\r\n]*))?$/);
  return commands.find(
    (command) => command.name === match?.[1] && (command.name === "/rename" || !match?.[2]?.trim()),
  );
}
