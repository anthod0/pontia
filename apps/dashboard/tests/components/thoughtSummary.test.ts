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

  const button = screen.getByRole('button', { name: 'View thought details' });
  const stepCount = screen.getByText('Thought for 3 steps');
  const brainIcon = button.querySelector('svg');

  expect(button).toHaveTextContent('Thought for 3 steps');
  expect(button).toHaveClass('py-2.5');
  expect(stepCount).toHaveClass('text-xs', 'leading-4');
  expect(brainIcon).toHaveClass('size-5');
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('Thinking in progress')).not.toBeInTheDocument();
});

test('busy thought summary shows the step count above the latest step with a loading icon', () => {
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
  const stepCount = screen.getByText('Thought for 3 steps');
  const latestStep = screen.getByText('read');

  expect(loadingIcon).toBeInTheDocument();
  expect(stepCount).toHaveClass('text-xs');
  expect(stepCount.compareDocumentPosition(latestStep) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByText('Planning changes')).not.toBeInTheDocument();
  expect(screen.queryByText('bash')).not.toBeInTheDocument();
});

test('busy thought summary aligns the loading icon with the step count header', () => {
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
  const header = loadingIcon.parentElement;

  expect(header).toHaveClass('items-center');
  expect(header).toHaveTextContent('Thought for 3 steps');
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
