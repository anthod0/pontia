<script lang="ts">
  import { t } from '$lib/i18n'
  import { managedToolUseTitle, type SessionChatThoughtStep } from '$lib/session-chat/sessionChat'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { useFilePreview } from '../../../components/chat/filePreviewContext'
  import { useDeviceRuntime } from '../../../device/context'
  import type { SessionView, TurnView } from '../../../api/types'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import TerminalIcon from 'phosphor-svelte/lib/TerminalIcon'
  import TurnDuration from '$lib/components/session-chat/TurnDuration.svelte'
  import MobileStepOutputSheet from './MobileStepOutputSheet.svelte'
  import MobileStepDiffScreen from './MobileStepDiffScreen.svelte'
  let {
    steps,
    active,
    turn,
    session,
  }: { steps: SessionChatThoughtStep[]; active: boolean; turn?: TurnView; session: SessionView } = $props()
  const device = useDeviceRuntime(),
    stack = useMobileStack()!,
    preview = useFilePreview()
  let expanded = $state(false),
    outputId = $state(''),
    path = $state(''),
    patch = $state('')
  const output = $derived(steps.find((step) => step.id === outputId))
  function showDiff(next: string, content = '') {
    path = next
    patch = content
    stack.open({ content: diff, title: path })
  }
  function open(step: SessionChatThoughtStep) {
    const input = step.managedToolUse?.input
    if (input?.type === 'read')
      preview?.open({
        device,
        sessionId: session.session_id,
        path: input.path.startsWith('/')
          ? input.path
          : (session.workspace ? session.workspace.replace(/\/$/, '') + '/' : '') + input.path,
      })
    else if (input?.type === 'edit' || input?.type === 'write') showDiff(input.path)
    else outputId = step.id
  }
</script>

{#snippet diff()}<MobileStepDiffScreen
    {path}
    {patch}
    workspace={session.workspace}
    workspaceId={session.workspace_id}
  />{/snippet}
<div class="my-2 overflow-hidden rounded-xl border bg-card">
  <button
    class="flex min-h-12 w-full items-center gap-2 px-3 text-left text-sm"
    aria-expanded={expanded}
    onclick={() => (expanded = !expanded)}
    ><TerminalIcon /><span class="min-w-0 flex-1 truncate"
      >{active
        ? $t('common.running') + ' · ' + (steps.at(-1)?.title ?? '')
        : $t('chat.activitySteps', { count: steps.length })}</span
    ><TurnDuration {turn} {active} /><CaretRightIcon class={expanded ? 'rotate-90' : ''} /></button
  >
  {#if expanded}{#each steps as step (step.id)}
      {@const input = step.managedToolUse?.input}
      {@const title = step.managedToolUse ? managedToolUseTitle(step.managedToolUse, $t) : step.title}
      {#if input?.type === 'file_changes'}{#each input.changes as change, i (i)}<button
            class="flex min-h-12 w-full flex-col justify-center border-t px-3 text-left"
            onclick={() => showDiff(change.path, change.diff)}
            ><strong>{title}</strong><small class="w-full truncate font-mono">{change.path}</small></button
          >{/each}
      {:else}<button
          class="flex min-h-12 w-full flex-col justify-center border-t px-3 text-left"
          onclick={() => open(step)}
          ><strong>{title}</strong><small class="w-full truncate font-mono"
            >{input && 'path' in input ? input.path : step.content}</small
          ></button
        >{/if}
    {/each}{/if}
</div>
{#if output}<MobileStepOutputSheet step={output} close={() => (outputId = '')} />{/if}
