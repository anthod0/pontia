import { render, screen } from '@testing-library/svelte';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
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

test('idle thought summary renders no collapsed step count trigger', () => {
  render(ThoughtSummary, {
    props: {
      steps: [
        step({ id: 'thought-1', kind: 'thinking', title: 'Thinking' }),
        step({ id: 'thought-2', kind: 'tool_call', title: 'bash' }),
        step({ id: 'thought-3', kind: 'tool_result', title: 'bash result' }),
      ],
      active: false,
    },
  });

  expect(screen.queryByRole('button', { name: 'View thought details' })).not.toBeInTheDocument();
  expect(screen.queryByText('Thought for 3 steps')).not.toBeInTheDocument();
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
});

test('busy thought summary shows the latest step without the step count trigger', () => {
  render(ThoughtSummary, {
    props: {
      steps: [
        step({ id: 'thought-1', kind: 'thinking', title: 'Thinking', content: 'Planning changes' }),
        step({ id: 'thought-2', kind: 'tool_call', title: 'bash', content: 'rg ThoughtSummary' }),
        step({ id: 'thought-3', kind: 'tool_call', title: 'read', content: 'ThoughtSummary.svelte' }),
      ],
      active: true,
    },
  });

  const latestStep = screen.getByText('read');

  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
  expect(screen.queryByText('Thought for 3 steps')).not.toBeInTheDocument();
  expect(latestStep).toBeInTheDocument();
  expect(screen.queryByText('Planning changes')).not.toBeInTheDocument();
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
});

test('busy thought summary removes the step count header icon and text', () => {
  render(ThoughtSummary, {
    props: {
      steps: [
        step({ id: 'thought-1', kind: 'thinking', title: 'Thinking', content: 'Planning changes' }),
        step({ id: 'thought-2', kind: 'tool_call', title: 'bash', content: 'rg ThoughtSummary' }),
        step({ id: 'thought-3', kind: 'tool_call', title: 'read', content: 'ThoughtSummary.svelte' }),
      ],
      active: true,
    },
  });

  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
  expect(screen.queryByText('Thought for 3 steps')).not.toBeInTheDocument();
});

test('busy thought summary removes the blocks wave header spinner', () => {
  render(ThoughtSummary, {
    props: {
      steps: [step({ id: 'thought-1', kind: 'thinking', title: 'Thinking', content: 'Planning changes' })],
      active: true,
    },
  });

  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
  expect(screen.queryByTestId('blocks-wave-spinner')).not.toBeInTheDocument();
});

test('blocks wave spinner preserves the original per-block wave position animations', () => {
  const source = readFileSync(join(process.cwd(), 'src/lib/components/session-chat/BlocksWaveSpinner.svelte'), 'utf8');

  expect(source).not.toContain('.blocks-wave-spinner :global(rect) {\n    animation: blocks-wave-size 1.2s linear infinite;');
  expect(source).toContain('animation: blocks-wave-size 1.2s linear infinite, blocks-wave-pos-1 1.2s linear infinite;');
  expect(source).toContain('animation: blocks-wave-size 1.2s linear infinite, blocks-wave-pos-9 1.2s linear infinite;');
});
