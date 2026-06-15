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

test('idle thought summary collapses to a step count', () => {
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

  expect(screen.getByRole('button', { name: 'View thought details' })).toHaveTextContent('Thought for 3 steps');
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
});

test('busy thought summary shows the latest step with a loading icon', () => {
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

  expect(screen.queryByText('Thought for 3 steps')).not.toBeInTheDocument();
  expect(screen.getByLabelText('Thinking in progress')).toBeInTheDocument();
  expect(screen.queryByText('Planning changes')).not.toBeInTheDocument();
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.getByText('read')).toBeInTheDocument();
});

test('busy thought summary top-aligns the loading icon with the recent steps', () => {
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

  const loadingIcon = screen.getByLabelText('Thinking in progress');
  expect(loadingIcon.parentElement).toHaveClass('items-start');
  expect(loadingIcon.parentElement).not.toHaveClass('items-center');
});

test('busy thought summary uses the blocks wave spinner for the main loading icon', () => {
  render(ThoughtSummary, {
    props: {
      steps: [step({ id: 'thought-1', kind: 'thinking', title: 'Thinking', content: 'Planning changes' })],
      active: true,
    },
  });

  const loadingIcon = screen.getByLabelText('Thinking in progress');
  expect(loadingIcon).toContainElement(screen.getByTestId('blocks-wave-spinner'));
  expect(loadingIcon).toHaveClass('text-muted-foreground');
  expect(loadingIcon).not.toHaveClass('text-primary');
});

test('blocks wave spinner preserves the original per-block wave position animations', () => {
  const source = readFileSync(join(process.cwd(), 'src/lib/components/session-chat/BlocksWaveSpinner.svelte'), 'utf8');

  expect(source).not.toContain('.blocks-wave-spinner :global(rect) {\n    animation: blocks-wave-size 1.2s linear infinite;');
  expect(source).toContain('animation: blocks-wave-size 1.2s linear infinite, blocks-wave-pos-1 1.2s linear infinite;');
  expect(source).toContain('animation: blocks-wave-size 1.2s linear infinite, blocks-wave-pos-9 1.2s linear infinite;');
});
