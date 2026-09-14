import { describe, expect, test } from 'vitest';
import { promptDocumentFromText, promptTextFromDocument } from '../src/lib/file-picker/fileMention';

describe('file mention prompt documents', () => {
  test('converts text and newlines to a minimal Tiptap document', () => {
    const document = promptDocumentFromText('inspect src/main.rs\nthen report');

    expect(document).toEqual({
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [
          { type: 'text', text: 'inspect src/main.rs' },
          { type: 'hardBreak' },
          { type: 'text', text: 'then report' },
        ],
      }],
    });
    expect(promptTextFromDocument(document)).toBe('inspect src/main.rs\nthen report');
  });

  test('represents boundary-delimited file references as atomic mention nodes without converting email addresses', () => {
    const knownFiles = new Map([['src/main.rs', { path: 'src/main.rs', name: 'main.rs', kind: 'file' }]]);
    const document = promptDocumentFromText('email test@example.com; inspect (@src/main.rs)', knownFiles);
    const mention = document.content?.[0]?.content?.find((node) => node.type === 'mention');

    expect(mention?.attrs).toEqual({ path: 'src/main.rs', name: 'main.rs', kind: 'file' });
    expect(document.content?.[0]?.content).toContainEqual({ type: 'text', text: 'email test@example.com; inspect (' });
    expect(document.content?.[0]?.content).toContainEqual({ type: 'text', text: ')' });
    expect(promptTextFromDocument(document)).toBe('email test@example.com; inspect (@src/main.rs)');
  });

  test('keeps unknown @ tokens as editable text', () => {
    const document = promptDocumentFromText('review @alice and @src/main.rs');

    expect(document.content?.[0]?.content).toEqual([
      { type: 'text', text: 'review @alice and @src/main.rs' },
    ]);
  });

  test('restores API identities for paths containing spaces', () => {
    const knownFiles = new Map([['docs/design notes', { path: 'docs/design notes', name: 'design notes', kind: 'directory' }]]);
    const document = promptDocumentFromText('inspect @docs/design notes next', knownFiles);

    expect(document.content?.[0]?.content).toContainEqual({
      type: 'mention',
      attrs: { path: 'docs/design notes', name: 'design notes', kind: 'directory' },
    });
    expect(promptTextFromDocument(document)).toBe('inspect @docs/design notes next');
  });

  test('serializes mention nodes as the existing plain-text prompt format', () => {
    expect(promptTextFromDocument({
      type: 'doc',
      content: [{
        type: 'paragraph',
        content: [
          { type: 'text', text: 'open ' },
          { type: 'mention', attrs: { path: 'src/main.rs', name: 'main.rs', kind: 'file' } },
          { type: 'text', text: ' now' },
        ],
      }],
    })).toBe('open @src/main.rs now');
  });
});
