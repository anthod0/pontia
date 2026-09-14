import type { JSONContent } from '@tiptap/core';
import type { FilePickerFileView } from '../../api/types';

const TOKEN_BOUNDARY = /[\s()[\]{}<>"'`]/;
const NO_KNOWN_FILES: ReadonlyMap<string, FilePickerFileView> = new Map();

export function isFileMentionBoundary(character: string | null | undefined): boolean {
  return character == null || TOKEN_BOUNDARY.test(character);
}

export interface FileMentionAttributes {
  path: string;
  name: string;
  kind: string;
}

export function fileMentionAttributes(file: FilePickerFileView): FileMentionAttributes {
  return { path: file.path, name: file.name, kind: file.kind };
}

function mentionAt(value: string, start: number, knownFiles: ReadonlyMap<string, FilePickerFileView>): { end: number; attributes: FileMentionAttributes } | null {
  if (value[start] !== '@' || !isFileMentionBoundary(value[start - 1])) return null;
  const afterTrigger = value.slice(start + 1);
  const known = [...knownFiles.values()]
    .filter((file) => afterTrigger.startsWith(file.path) && isFileMentionBoundary(afterTrigger[file.path.length]))
    .sort((a, b) => b.path.length - a.path.length)[0];
  const path = known?.path;
  if (!path) return null;
  return {
    end: start + path.length + 1,
    attributes: {
      path,
      name: known?.name ?? path.split('/').at(-1) ?? path,
      kind: known?.kind ?? 'file',
    },
  };
}

function inlineContentFromText(value: string, knownFiles: ReadonlyMap<string, FilePickerFileView>): JSONContent[] {
  const content: JSONContent[] = [];
  let textStart = 0;

  for (let index = 0; index < value.length;) {
    const mention = mentionAt(value, index, knownFiles);
    if (!mention) {
      index += 1;
      continue;
    }
    if (index > textStart) content.push({ type: 'text', text: value.slice(textStart, index) });
    content.push({ type: 'mention', attrs: mention.attributes });
    index = mention.end;
    textStart = index;
  }

  if (textStart < value.length) content.push({ type: 'text', text: value.slice(textStart) });
  return content;
}

export function promptDocumentFromText(value: string, knownFiles: ReadonlyMap<string, FilePickerFileView> = NO_KNOWN_FILES): JSONContent {
  const content: JSONContent[] = [];
  const lines = value.split('\n');
  const paragraphContent: JSONContent[] = [];

  lines.forEach((line, index) => {
    paragraphContent.push(...inlineContentFromText(line, knownFiles));
    if (index < lines.length - 1) paragraphContent.push({ type: 'hardBreak' });
  });
  content.push({ type: 'paragraph', ...(paragraphContent.length ? { content: paragraphContent } : {}) });

  return { type: 'doc', content };
}

export function promptTextFromDocument(document: JSONContent): string {
  const blocks: string[] = [];

  for (const block of document.content ?? []) {
    let text = '';
    for (const node of block.content ?? []) {
      if (node.type === 'text') text += node.text ?? '';
      else if (node.type === 'hardBreak') text += '\n';
      else if (node.type === 'mention') text += `@${String(node.attrs?.path ?? '')}`;
    }
    blocks.push(text);
  }

  return blocks.join('\n');
}
