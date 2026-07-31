import { fireEvent, render, screen, within } from '@testing-library/svelte'
import { describe, expect, test, vi } from 'vitest'
import ApprovalRequestCard from '../src/lib/components/session-chat/ApprovalRequestCard.svelte'

const suggestion = {
  type: 'addRules',
  rules: [{ toolName: 'Bash', ruleContent: 'pnpm test' }],
  behavior: 'allow',
  destination: 'localSettings',
}

const approval = {
  requestEventId: 'evt-approval',
  toolName: 'Bash',
  permissionSuggestions: [suggestion],
}

describe('ApprovalRequestCard', () => {
  test('uses a white request surface and separates suggestion actions from current-request decisions', () => {
    render(ApprovalRequestCard, { props: { approval, onDecision: vi.fn() } })

    expect(screen.getByLabelText('Approval required for Bash')).toHaveClass('bg-white')
    const decisions = screen.getByRole('group', { name: 'Current request decisions' })
    expect(within(decisions).getByRole('button', { name: 'Accept Once' })).toHaveClass('bg-white')
    expect(within(decisions).getByRole('button', { name: 'Reject' })).toBeInTheDocument()
    expect(within(decisions).queryByRole('button', { name: 'Always Allow' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Always Allow' })).toHaveClass('bg-white')
  })

  test('gives every permission suggestion its own Always Allow action', async () => {
    const alternativeSuggestion = { ...suggestion, destination: 'userSettings' }
    const onDecision = vi.fn(async () => undefined)
    render(ApprovalRequestCard, {
      props: {
        approval: { ...approval, permissionSuggestions: [suggestion, alternativeSuggestion] },
        onDecision,
      },
    })

    const alwaysAllow = screen.getAllByRole('button', { name: 'Always Allow' })
    expect(alwaysAllow).toHaveLength(2)
    await fireEvent.click(alwaysAllow[1])
    expect(onDecision).toHaveBeenCalledWith({
      decision: 'always_allow',
      permission_suggestion: alternativeSuggestion,
    })
  })

  test.each([
    ['Accept Once', { decision: 'accept_once' }],
    ['Reject', { decision: 'reject' }],
    ['Always Allow', { decision: 'always_allow', permission_suggestion: suggestion }],
  ] as const)('submits the exact %s decision', async (buttonName, expected) => {
    const onDecision = vi.fn(async () => undefined)
    render(ApprovalRequestCard, { props: { approval, onDecision } })

    await fireEvent.click(screen.getByRole('button', { name: buttonName }))

    expect(onDecision).toHaveBeenCalledTimes(1)
    expect(onDecision).toHaveBeenCalledWith(expected)
    expect(screen.queryByText(/Decision delivered|Waiting for a decision/)).not.toBeInTheDocument()
  })

  test('disables every decision while a command is in flight', async () => {
    let release!: () => void
    const pending = new Promise<void>((resolve) => {
      release = resolve
    })
    const onDecision = vi.fn(() => pending)
    render(ApprovalRequestCard, { props: { approval, onDecision } })

    const accept = screen.getByRole('button', { name: 'Accept Once' })
    await fireEvent.click(accept)
    await fireEvent.click(accept)

    expect(onDecision).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('button', { name: 'Accept Once' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Reject' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Always Allow' })).toBeDisabled()
    release()
    await pending
  })

  test('shows command failures and re-enables every decision', async () => {
    const onDecision = vi.fn(async () => {
      throw new Error('Approval request is no longer actionable')
    })
    render(ApprovalRequestCard, { props: { approval, onDecision } })

    await fireEvent.click(screen.getByRole('button', { name: 'Reject' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('Approval request is no longer actionable')
    expect(screen.getByRole('button', { name: 'Accept Once' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Reject' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Always Allow' })).toBeEnabled()
  })
})
