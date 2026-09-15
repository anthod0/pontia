import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { expect, test } from 'vitest';
import ThoughtSummary from '../../src/lib/components/session-chat/ThoughtSummary.svelte';
import type { SessionChatThoughtStep } from '../../src/lib/session-chat/sessionChat';

function step(overrides: Partial<SessionChatThoughtStep> = {}): SessionChatThoughtStep {
  return {
    id: 'thought-1',
    kind: 'thinking',
    title: 'Thinking',
    status: null,
    content: 'Inspecting the project.',
    occurredAt: '2026-06-11T00:00:00Z',
    ...overrides,
  };
}

test('completed work expands inline from its summary heading', async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      steps: [
        step({ id: 'thought-1', content: 'Inspecting the project.' }),
        step({ id: 'assistant-1', kind: 'assistant', title: 'Assistant update', content: 'I found the **relevant** component.' }),
        step({ id: 'thought-2', kind: 'tool_call', title: 'Read file', content: 'AGENTS.md', managedToolUse: { tool_name: 'read', input: { type: 'read', path: 'AGENTS.md' } } }),
        step({ id: 'unknown-1', kind: 'tool_call', title: 'Custom tool', content: 'secret input' }),
        step({ id: 'result-1', kind: 'tool_result', title: 'Read file', content: 'File contents' }),
      ],
    },
  });

  const trigger = screen.getByRole('button', { name: 'Show agent work steps' });
  expect(trigger).toHaveTextContent('Agent work');
  expect(screen.getByText('Inspecting the project.')).not.toBeVisible();

  await user.click(trigger);

  expect(screen.getByRole('button', { name: 'Hide agent work steps' })).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByLabelText('Thinking')).toBeInTheDocument();
  expect(screen.getByText('Inspecting the project.')).toBeInTheDocument();
  expect(screen.getByText('relevant').tagName).toBe('STRONG');
  expect(screen.getByText('Read 1 file')).toBeInTheDocument();
  expect(screen.getByText('AGENTS.md')).not.toBeVisible();
  expect(screen.getByText('Custom tool')).toBeInTheDocument();
  expect(screen.getByText('secret input')).not.toBeVisible();
  expect(screen.queryByText('File contents')).not.toBeInTheDocument();

  await user.click(screen.getByRole('button', { name: 'Show Custom tool parameters' }));

  expect(screen.getByRole('button', { name: 'Hide Custom tool parameters' })).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByText('secret input')).toBeInTheDocument();
});

test('groups adjacent file operations by type and reveals their file lists', async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      active: true,
      steps: [
        step({ id: 'read-1', kind: 'tool_call', title: 'Read file', content: 'a.ts', managedToolUse: { tool_name: 'read', input: { type: 'read', path: 'a.ts' } } }),
        step({ id: 'read-2', kind: 'tool_call', title: 'Read file', content: 'b.ts', managedToolUse: { tool_name: 'read', input: { type: 'read', path: 'b.ts' } } }),
        step({ id: 'write-1', kind: 'tool_call', title: 'Write file', content: 'c.ts', managedToolUse: { tool_name: 'write', input: { type: 'write', path: 'c.ts' } } }),
        step({ id: 'write-2', kind: 'tool_call', title: 'Write file', content: 'd.ts', managedToolUse: { tool_name: 'write', input: { type: 'write', path: 'd.ts' } } }),
        step({ id: 'edit-1', kind: 'tool_call', title: 'Edit file', content: 'e.ts', managedToolUse: { tool_name: 'edit', input: { type: 'edit', path: 'e.ts', edits_count: 1 } } }),
        step({ id: 'edit-2', kind: 'tool_call', title: 'Edit file', content: 'f.ts', managedToolUse: { tool_name: 'edit', input: { type: 'edit', path: 'f.ts', edits_count: 1 } } }),
      ],
    },
  });

  expect(screen.getByText('Read 2 files')).toBeInTheDocument();
  expect(screen.getByText('Write 2 files')).toBeInTheDocument();
  expect(screen.getByText('Edit 2 files')).toBeInTheDocument();
  expect(screen.getByText('a.ts')).not.toBeVisible();

  await user.click(screen.getByRole('button', { name: 'Show Read 2 files details' }));

  expect(screen.getByRole('button', { name: 'Hide Read 2 files details' })).toHaveAttribute('aria-expanded', 'true');
  expect(screen.getByText('a.ts')).toBeInTheDocument();
  expect(screen.getByText('b.ts')).toBeInTheDocument();
});

test('run command reveals its command from a nested disclosure', async () => {
  const user = userEvent.setup();
  render(ThoughtSummary, {
    props: {
      active: true,
      steps: [
        step({
          id: 'bash-1',
          kind: 'tool_call',
          title: 'Run command',
          content: 'pnpm test',
          managedToolUse: { tool_name: 'bash', input: { type: 'bash', command: 'pnpm test' } },
        }),
      ],
    },
  });

  const trigger = screen.getByRole('button', { name: 'Show Run command details' });
  expect(document.querySelector('.command-code')).not.toBeVisible();

  await user.click(trigger);

  expect(screen.getByRole('button', { name: 'Hide Run command details' })).toHaveAttribute('aria-expanded', 'true');
  expect(document.querySelector('.command-code')).toBeVisible();
  expect(document.querySelector('.command-code')).toHaveTextContent('pnpm test');
});

test('active work is expanded inline by default', () => {
  render(ThoughtSummary, {
    props: {
      steps: [
        step({ id: 'thought-1', content: 'Planning changes.' }),
        step({ id: 'thought-2', kind: 'tool_call', title: 'Read file', content: 'ThoughtSummary.svelte', managedToolUse: { tool_name: 'read', input: { type: 'read', path: 'ThoughtSummary.svelte' } } }),
      ],
      active: true,
    },
  });

  expect(screen.getByRole('button', { name: 'Hide agent work steps' })).toHaveTextContent('Working');
  expect(screen.getByText('Planning changes.')).toBeInTheDocument();
  expect(screen.getByText('Read 1 file')).toBeInTheDocument();
  expect(screen.getByText('ThoughtSummary.svelte')).not.toBeVisible();
});
