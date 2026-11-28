<script lang="ts">
  import { t } from '$lib/i18n'
  import { managedToolUseTitle, type SessionChatThoughtStep } from '$lib/session-chat/sessionChat'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import { useFilePreview } from '../../../components/chat/filePreviewContext'
  import { useDeviceRuntime } from '../../../device/context'
  import type { SessionView } from '../../../api/types'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import FileMagnifyingGlassIcon from 'phosphor-svelte/lib/FileMagnifyingGlassIcon'
  import FilePlusIcon from 'phosphor-svelte/lib/FilePlusIcon'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import LightbulbIcon from 'phosphor-svelte/lib/LightbulbIcon'
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon'
  import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon'
  import WrenchIcon from 'phosphor-svelte/lib/WrenchIcon'
  import MobileStepOutputSheet from './MobileStepOutputSheet.svelte'
  import MobileStepDiffScreen from './MobileStepDiffScreen.svelte'
  let {
    steps,
    active,
    session,
  }: { steps: SessionChatThoughtStep[]; active: boolean; session: SessionView } = $props()
  const device = useDeviceRuntime(),
    stack = useMobileStack()!,
    preview = useFilePreview()
  let expanded = $state(false),
    outputId = $state(''),
    path = $state(''),
    patch = $state('')
  const output = $derived(steps.find((step) => step.id === outputId))
  const latestStep = $derived(steps.at(-1))
  function title(step: SessionChatThoughtStep) {
    const input = step.managedToolUse?.input
    if (input?.type === 'read' || input?.type === 'write' || input?.type === 'edit')
      return $t('chat.fileCount', {
        operation: $t(input.type === 'read' ? 'files.read' : input.type === 'write' ? 'files.write' : 'common.edit'),
        count: 1,
      })
    if (input?.type === 'file_changes')
      return $t('chat.fileCount', { operation: $t('files.change'), count: input.changes.length })
    return step.managedToolUse ? managedToolUseTitle(step.managedToolUse, $t) : step.title
  }
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
<div class="not-prose min-w-0" data-chat-activity>
  <button
    class="group inline-flex min-w-0 max-w-full touch-target:min-h-11 items-center gap-1.5 py-1 pr-3 text-sm leading-5 text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
    aria-label={expanded ? $t('chat.hideSteps') : $t('chat.showSteps')}
    aria-expanded={expanded}
    onclick={() => (expanded = !expanded)}
  >
    {#if active && latestStep}
      <SpinnerGapIcon class="size-3.5 shrink-0 animate-spin" aria-hidden="true" />
      <span class="truncate">{title(latestStep)}</span>
    {:else}
      <span class="truncate">{$t('chat.activitySteps', { count: steps.length })}</span>
    {/if}
    <CaretRightIcon class={['size-4 shrink-0 transition-transform', expanded && 'rotate-90']} aria-hidden="true" />
  </button>
  {#if expanded}
    <div class="space-y-1 pb-2 pt-1">
      {#each steps as step, index (step.id)}
        {@const input = step.managedToolUse?.input}
        {#if input?.type === 'file_changes'}
          {#each input.changes as change, changeIndex (changeIndex)}
            <button
              class="relative flex w-full min-w-0 touch-target:min-h-11 gap-3 py-1.5 text-left"
              onclick={() => showDiff(change.path, change.diff)}
            >
              <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground"><FileTextIcon class="size-4" aria-hidden="true" /></span>
              <span class="min-w-0 flex-1 break-words pt-0.5 text-sm font-medium leading-5 text-foreground/75">{title(step)} · {change.path}</span>
              {#if index < steps.length - 1 || changeIndex < input.changes.length - 1}<span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>{/if}
            </button>
          {/each}
        {:else}
          <button
            class="relative flex w-full min-w-0 touch-target:min-h-11 gap-3 py-1.5 text-left"
            onclick={() => open(step)}
          >
            <span class="relative z-10 flex size-6 shrink-0 items-center justify-center bg-background text-muted-foreground">
              {#if step.kind === 'thinking'}
                <LightbulbIcon class="size-4" aria-hidden="true" />
              {:else if input?.type === 'bash'}
                <TerminalWindowIcon class="size-4" aria-hidden="true" />
              {:else if input?.type === 'read'}
                <FileMagnifyingGlassIcon class="size-4" aria-hidden="true" />
              {:else if input?.type === 'write'}
                <FilePlusIcon class="size-4" aria-hidden="true" />
              {:else if input?.type === 'edit'}
                <FileTextIcon class="size-4" aria-hidden="true" />
              {:else}
                <WrenchIcon class="size-4" aria-hidden="true" />
              {/if}
            </span>
            <span class="min-w-0 flex-1 break-words pt-0.5 text-sm font-medium leading-5 text-foreground/75">{title(step)}</span>
            {#if index < steps.length - 1}<span class="absolute bottom-[-0.625rem] left-[0.71875rem] top-6 w-px bg-border" aria-hidden="true"></span>{/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>
{#if output}<MobileStepOutputSheet step={output} close={() => (outputId = '')} />{/if}
