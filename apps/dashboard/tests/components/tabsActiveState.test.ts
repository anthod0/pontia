import { render, screen } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import TabsHost from './TabsHost.svelte';

test('tabs trigger styling targets the selected aria state emitted by bits-ui', () => {
  render(TabsHost);

  const activeTab = screen.getByRole('tab', { name: 'Active', selected: true });
  expect(activeTab).toHaveClass(/aria-selected:bg-background/);
});

test('tabs triggers do not render per-button border outlines in default tabs', () => {
  render(TabsHost);

  for (const tab of screen.getAllByRole('tab')) {
    expect(tab).not.toHaveClass(/(?:^|\s)border(?:\s|$)/);
    expect(tab).not.toHaveClass(/border-transparent|aria-selected:border-input|dark:aria-selected:border-input/);
  }
});
