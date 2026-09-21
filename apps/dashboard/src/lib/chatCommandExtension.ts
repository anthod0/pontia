import { Extension } from '@tiptap/core';
import { PluginKey } from '@tiptap/pm/state';
import Suggestion, { exitSuggestion, type SuggestionOptions, type SuggestionProps } from '@tiptap/suggestion';
import type { ChatCommand } from './chatCommands';

interface CommandSelection {
  command: ChatCommand;
  execute: boolean;
}

export type ChatCommandSuggestion = SuggestionProps<ChatCommand, CommandSelection>;

export function createChatCommandExtension(options: {
  pluginKey: PluginKey;
  commands: () => ChatCommand[];
  onCommand: (command: ChatCommand) => void;
  render: SuggestionOptions<ChatCommand, CommandSelection>['render'];
}) {
  return Extension.create({
    name: 'chatCommandSuggestion',
    priority: 200,
    addProseMirrorPlugins() {
      return [Suggestion<ChatCommand, CommandSelection>({
        editor: this.editor,
        pluginKey: options.pluginKey,
        char: '/',
        allowedPrefixes: [' ', '\t'],
        allow: ({ state, range }) => state.selection.empty
          && range.to === state.doc.content.size - 1
          && /^[\t ]*\/[a-z]*$/.test(state.doc.textBetween(0, state.doc.content.size, '\n', '\n'))
          && options.commands().some((command) => command.name.startsWith(state.doc.textBetween(range.from, range.to))),
        command: ({ editor, range, props }) => {
          const command = options.commands().find((item) => item.name === props.command.name);
          if (!editor.isEditable || !command || command.disabledReason) return;
          exitSuggestion(editor.view, options.pluginKey);
          editor.chain().focus().insertContentAt(range, command.name === '/rename' ? '/rename ' : command.name).run();
          if (props.execute && command.name !== '/rename') options.onCommand(command);
        },
        render: options.render,
      })];
    },
  });
}
