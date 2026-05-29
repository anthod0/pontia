import { render, screen } from '@testing-library/svelte';
import { expect, test } from 'vitest';
import TabsHost from './TabsHost.svelte';

test('tabs trigger styling targets the selected aria state emitted by bits-ui', () => {
  render(TabsHost);

  const activeTab = screen.getByRole('tab', { name: 'Active', selected: true });
  expect(activeTab).toHaveClass(/aria-selected:bg-background/);
});
