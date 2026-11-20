<script lang="ts">
  import { t } from '$lib/i18n';
  import { clientSessionLabel } from '../../clients'
  import AtIcon from 'phosphor-svelte/lib/AtIcon'
  import FolderIcon from 'phosphor-svelte/lib/FolderIcon'
  import GaugeIcon from 'phosphor-svelte/lib/GaugeIcon'
  import GitBranchIcon from 'phosphor-svelte/lib/GitBranchIcon'
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import { Badge } from '$lib/components/ui/badge/index.js'
  import type { SessionView, WorkspaceGitStatusView, WorkspaceView } from '../../api/types'
  import GitStatusInline from './GitStatusInline.svelte'
  import { gitStatusAriaLabel, gitStatusTitle, sessionContextUsageLabel, sessionHandleTitle, sessionWorkspacePath, sessionWorkspaceTitle } from './sessionMetadata'

  interface Props {
    session: SessionView
    gitStatus?: WorkspaceGitStatusView
    gitStatusErrors?: Record<string, string | null | undefined>
    workspaces: WorkspaceView[]
  }

  let { session, gitStatus, gitStatusErrors = {}, workspaces }: Props = $props()
</script>

<Badge variant="outline" class="h-7 max-w-full justify-start gap-1.5 px-3 text-sm font-normal text-muted-foreground" title={$t('chat.workspaceValue', { value: sessionWorkspacePath(session, workspaces, $t) })} aria-label={$t('chat.workspaceValue', { value: sessionWorkspacePath(session, workspaces, $t) })}>
  <FolderIcon class="size-4" aria-hidden="true" />
  <span class="min-w-0 truncate">{sessionWorkspaceTitle(session, workspaces, $t)}</span>
</Badge>
{#if gitStatus}
  <Badge
    variant={gitStatus.state === 'error' ? 'destructive' : 'outline'}
    class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground"
    title={gitStatusTitle(session, gitStatus, gitStatusErrors, $t)}
    aria-label={gitStatusAriaLabel(gitStatus, $t)}
  >
    <GitBranchIcon class="size-4" aria-label={$t('git.branch')} />
    <GitStatusInline {gitStatus} />
  </Badge>
{/if}
{#if sessionContextUsageLabel(session, $t)}
  <Badge variant="outline" class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground" title={$t('context.usage', { usage: sessionContextUsageLabel(session, $t) })} aria-label={$t('context.usage', { usage: sessionContextUsageLabel(session, $t) })}>
    <GaugeIcon class="size-4" aria-hidden="true" /> {sessionContextUsageLabel(session, $t)}
  </Badge>
{/if}
<Badge variant="outline" class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground" title={$t('chat.clientValue', { value: clientSessionLabel(session) })} aria-label={$t('chat.clientValue', { value: clientSessionLabel(session) })}>
  <TerminalWindowIcon class="size-4" aria-hidden="true" /> {clientSessionLabel(session)}
</Badge>
{#if sessionHandleTitle(session)}
  <Badge variant="outline" class="h-7 gap-1.5 px-3 text-sm font-normal text-muted-foreground" title={$t('chat.handleValue', { value: sessionHandleTitle(session) })} aria-label={$t('chat.handleValue', { value: sessionHandleTitle(session) })}>
    <AtIcon class="size-4" aria-hidden="true" /> {sessionHandleTitle(session)}
  </Badge>
{/if}
