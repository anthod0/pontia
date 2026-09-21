export interface ChatCommand {
  name: '/new' | '/rename' | '/exit';
  description: string;
  disabledReason?: string;
  run: () => void;
}

export function chatCommandQuery(value: string): string | null {
  return /^[\t ]*\/[a-z]*[\t ]*$/.test(value) ? value.trim() : null;
}

export function findChatCommand(value: string, commands: ChatCommand[]): ChatCommand | undefined {
  const query = chatCommandQuery(value);
  return commands.find((command) => command.name === query);
}
