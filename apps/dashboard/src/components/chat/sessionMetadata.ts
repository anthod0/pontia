import { contextUsageSummary } from '$lib/contextUsage'
import type { InboxMessageView, SessionView, WorkspaceGitStatusView, WorkspaceView } from '../../api/types'

export type SessionMetadataItem = {
  key: string
  label: string
  value: string
  title: string
}

export function workspaceTitle(workspace: WorkspaceView): string {
  return workspace.name ?? workspace.display_path ?? workspace.workspace_id
}

export function clientTitle(clientType: string): string {
  return clientType || 'Client'
}

export function sessionContextUsageLabel(session: SessionView): string | null {
  if ((session.capabilities?.context_usage ?? 'unsupported') === 'unsupported') return null
  const summary = session.context_usage ? contextUsageSummary(session.context_usage, { includeConfidence: false }) : 'Context waiting…'
  return summary.replace(/^Context\s+/, '')
}

export function sessionStateBadgeClass(state: string): string {
  switch (state) {
    case 'busy':
    case 'starting':
      return 'border-blue-500/30 bg-blue-500/10 text-blue-700 dark:text-blue-300'
    case 'idle':
      return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
    case 'interrupted':
      return 'border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300'
    case 'exited':
      return 'border-muted-foreground/25 bg-muted text-muted-foreground'
    case 'error':
      return 'border-destructive/30 bg-destructive/10 text-destructive'
    default:
      return ''
  }
}

export function sessionProfileTitle(session: SessionView): string | null {
  if (!session.execution_profile_id) return null
  return session.execution_profile_version ? `${session.execution_profile_id}@${session.execution_profile_version}` : session.execution_profile_id
}

export function sessionHandleTitle(session: SessionView): string | null {
  const handle = session.handle?.trim()
  return handle || null
}

export function sessionWorkspace(session: SessionView, workspaces: WorkspaceView[]): WorkspaceView | null {
  if (!session.workspace_id) return null
  return workspaces.find((workspace) => workspace.workspace_id === session.workspace_id) ?? null
}

export function sessionWorkspaceTitle(session: SessionView, workspaces: WorkspaceView[]): string {
  const workspace = sessionWorkspace(session, workspaces)
  return workspace?.name ?? workspace?.display_path ?? session.workspace ?? session.workspace_id ?? 'No workspace'
}

export function sessionWorkspacePath(session: SessionView, workspaces: WorkspaceView[]): string {
  const workspace = sessionWorkspace(session, workspaces)
  return workspace?.canonical_path ?? workspace?.display_path ?? session.workspace ?? session.workspace_id ?? 'No workspace'
}

export function gitStatusLabel(status: WorkspaceGitStatusView | undefined): string {
  if (!status || status.state === 'unknown') return 'Git unknown'
  if (status.state === 'error') return 'Git error'
  return status.clean ? 'clean' : 'dirty'
}

export function gitBranchLabel(status: WorkspaceGitStatusView | undefined): string {
  return status?.branch ?? 'No branch'
}

export function hasGitChangeCounts(status: WorkspaceGitStatusView | undefined): boolean {
  return !!status && (status.staged_count > 0 || status.unstaged_count > 0 || status.untracked_count > 0 || status.conflicted_count > 0 || status.ahead > 0 || status.behind > 0)
}

export function gitStatusCompactText(status: WorkspaceGitStatusView | undefined): string | null {
  if (!status) return null
  const parts = [gitBranchLabel(status)]
  if (status.ahead) parts.push(`↑${status.ahead}`)
  if (status.behind) parts.push(`↓${status.behind}`)
  if (hasGitChangeCounts(status)) {
    if (status.staged_count) parts.push(`+${status.staged_count}`)
    if (status.unstaged_count) parts.push(`~${status.unstaged_count}`)
    if (status.untracked_count) parts.push(`?${status.untracked_count}`)
    if (status.conflicted_count) parts.push(`!${status.conflicted_count}`)
  }
  return parts.join(' ')
}

export function gitStatusAriaLabel(status: WorkspaceGitStatusView | undefined): string {
  return `Git status: ${gitBranchLabel(status)}, ${gitStatusLabel(status)}`
}

export function gitStatusTitle(session: SessionView, status: WorkspaceGitStatusView | undefined, gitStatusErrors: Record<string, string | null | undefined>): string {
  const error = session.workspace_id ? gitStatusErrors[session.workspace_id] : null
  return status?.failure ?? error ?? gitStatusAriaLabel(status)
}

export function gitStatusToneClass(status: WorkspaceGitStatusView | undefined): string {
  if (!status || status.state === 'unknown') return 'text-muted-foreground'
  if (status.state === 'error') return 'text-destructive'
  return status.clean ? 'text-emerald-600 dark:text-emerald-400' : 'text-amber-600 dark:text-amber-400'
}

export function sessionMetadataItems(session: SessionView, workspaces: WorkspaceView[], gitStatus: WorkspaceGitStatusView | undefined, gitStatusErrors: Record<string, string | null | undefined>): SessionMetadataItem[] {
  const items: SessionMetadataItem[] = [
    { key: 'workspace', label: 'Workspace', value: sessionWorkspaceTitle(session, workspaces), title: sessionWorkspacePath(session, workspaces) },
    { key: 'client', label: 'Client', value: session.client_type, title: session.client_type },
  ]
  if (gitStatus) items.push({ key: 'git', label: 'Git', value: `${gitBranchLabel(gitStatus)} · ${gitStatusLabel(gitStatus)}`, title: gitStatusTitle(session, gitStatus, gitStatusErrors) })
  const contextUsageLabel = sessionContextUsageLabel(session)
  if (contextUsageLabel) items.push({ key: 'context', label: 'Usage', value: contextUsageLabel, title: `Context usage: ${contextUsageLabel}` })
  const profileTitle = sessionProfileTitle(session)
  if (profileTitle) items.push({ key: 'profile', label: 'Profile', value: profileTitle, title: profileTitle })
  const handleTitle = sessionHandleTitle(session)
  if (handleTitle) items.push({ key: 'handle', label: 'Handle', value: handleTitle, title: handleTitle })
  return items
}

export function sessionMetadataSummary(items: SessionMetadataItem[]): string {
  if (!items.length) return 'Session details'
  return items.map((item) => item.value).join(' · ')
}

export function visibleChatInboxMessages(messages: InboxMessageView[]): InboxMessageView[] {
  const actionable = messages.filter((message) => message.state === 'pending' || message.state === 'failed').slice().reverse()
  const dispatching = messages.filter((message) => message.state === 'dispatching').slice().reverse()
  return [...dispatching, ...actionable]
}

export function inboxBadgeVariant(message: InboxMessageView): 'default' | 'secondary' | 'destructive' | 'outline' {
  if (message.state === 'failed') return 'destructive'
  if (message.state === 'dispatching') return 'outline'
  return 'secondary'
}
