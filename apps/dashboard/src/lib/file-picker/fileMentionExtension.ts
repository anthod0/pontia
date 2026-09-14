import { mount, unmount } from 'svelte';
import { mergeAttributes } from '@tiptap/core';
import Mention from '@tiptap/extension-mention';
import type { PluginKey } from '@tiptap/pm/state';
import type { SuggestionProps } from '@tiptap/suggestion';
import type { FilePickerFileView } from '../../api/types';
import FileMentionChip from '../components/file-picker/FileMentionChip.svelte';
import { fileMentionAttributes, isFileMentionBoundary } from './fileMention';

interface FileMentionExtensionOptions {
  pluginKey: PluginKey;
  identities: Map<string, FilePickerFileView>;
  enabled: () => boolean;
  search: (query: string, signal: AbortSignal) => Promise<FilePickerFileView[]>;
  onSuggestion: (props: SuggestionProps) => void;
  onSuggestionExit: () => void;
}

export function createFileMentionExtension(options: FileMentionExtensionOptions) {
  const FileMention = Mention.extend({
    addAttributes() {
      return {
        ...this.parent?.(),
        path: {
          default: null,
          parseHTML: (element: HTMLElement) => element.getAttribute('data-path'),
          renderHTML: (attributes: Record<string, unknown>) => ({ 'data-path': attributes.path }),
        },
        name: {
          default: null,
          parseHTML: (element: HTMLElement) => element.getAttribute('data-name'),
          renderHTML: (attributes: Record<string, unknown>) => ({ 'data-name': attributes.name }),
        },
        kind: {
          default: 'file',
          parseHTML: (element: HTMLElement) => element.getAttribute('data-kind') ?? 'file',
          renderHTML: (attributes: Record<string, unknown>) => ({ 'data-kind': attributes.kind }),
        },
      };
    },
    addNodeView() {
      return ({ node }) => {
        const path = String(node.attrs.path ?? '');
        const kind = String(node.attrs.kind ?? 'file');
        const dom = document.createElement('span');
        dom.dataset.type = 'mention';
        dom.dataset.path = path;
        dom.dataset.name = String(node.attrs.name ?? '');
        dom.dataset.kind = kind;
        dom.title = path;
        dom.contentEditable = 'false';
        const component = mount(FileMentionChip, { target: dom, props: { path, kind } });
        return {
          dom,
          destroy: () => { void unmount(component); },
        };
      };
    },
  });

  return FileMention.configure({
    deleteTriggerWithBackspace: true,
    renderText: ({ node }) => `@${String(node.attrs.path ?? '')}`,
    renderHTML: ({ options: mentionOptions, node }) => [
      'span',
      mergeAttributes(mentionOptions.HTMLAttributes, {
        class: 'inline-flex max-w-full cursor-default items-center rounded-md bg-secondary px-1.5 py-0.5 align-baseline text-secondary-foreground',
        title: String(node.attrs.path ?? ''),
        'aria-label': `${node.attrs.kind === 'directory' ? 'Directory' : 'File'} ${String(node.attrs.path ?? '')}`,
      }),
      `@${String(node.attrs.path ?? '')}`,
    ],
    suggestion: {
      pluginKey: options.pluginKey,
      char: '@',
      allowedPrefixes: null,
      debounce: 150,
      allow: ({ state, range }) => {
        if (!options.enabled()) return false;
        const previous = state.doc.resolve(range.from).nodeBefore;
        return previous?.type.name === 'hardBreak'
          || isFileMentionBoundary(previous?.isText ? previous.text?.at(-1) : null);
      },
      items: ({ query, signal }) => options.search(query, signal),
      command: ({ editor, range, props }) => {
        const file = props as unknown as FilePickerFileView;
        options.identities.set(file.path, file);
        editor
          .chain()
          .focus()
          .insertContentAt(range, [
            { type: 'mention', attrs: fileMentionAttributes(file) },
            { type: 'text', text: ' ' },
          ])
          .run();
      },
      render: () => ({
        onStart: options.onSuggestion,
        onUpdate: options.onSuggestion,
        onExit: options.onSuggestionExit,
        onKeyDown: ({ event }) => {
          if (event.key !== 'Escape') return false;
          options.onSuggestionExit();
          return true;
        },
      }),
    },
  });
}
