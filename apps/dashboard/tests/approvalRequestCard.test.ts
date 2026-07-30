import { fireEvent, render, screen } from '@testing-library/svelte'
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
    expect(screen.getByText(/Decision delivered/)).toBeInTheDocument()
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
