<script lang="ts">
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import ArrowsClockwiseIcon from 'phosphor-svelte/lib/ArrowsClockwiseIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import XIcon from 'phosphor-svelte/lib/XIcon'
  import { Button } from '$lib/components/ui/button'
  import { Textarea } from '$lib/components/ui/textarea'
  import { t, type MessageKey } from '$lib/i18n'

  let { dirty, conflicted, branch, ahead, behind, touch, busy, pending, failure, dismiss, message = $bindable(), commit, sync }: {
    dirty: boolean; conflicted: boolean; branch: string; ahead: number; behind: number
    touch: boolean; busy: boolean; pending: string | null
    failure: { title: MessageKey; detail: string } | null; dismiss: () => void
    message: string; commit: () => void; sync: () => void
  } = $props()
  let input = $state<HTMLTextAreaElement | null>(null)
  let invalid = $state(false)
  const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)
  const committing = $derived(pending === 'commit' || (dirty && pending !== 'sync'))
  const show = $derived(dirty || ahead || behind || pending === 'commit' || pending === 'sync')

  $effect(() => { if (!committing) invalid = false })

  function submit() {
    if (busy || conflicted) return
    if (!committing) return sync()
    if (message.trim()) return commit()
    invalid = true
    input?.focus()
  }
</script>

{#if show || failure}
  <div class="git-commit-box px-2 pt-2 pb-1.5 touch-target:pt-3 touch-target:pb-2.5">
    {#if show}
      <Textarea bind:ref={input} bind:value={message} rows={1} disabled={busy}
        aria-label={$t('git.commitMessage')} aria-invalid={invalid}
        placeholder={conflicted ? $t('git.resolveConflictsFirst') : touch ? $t('git.commitMessage') : $t('git.commitPlaceholder', { branch, shortcut: mac ? '⌘↵' : 'Ctrl↵' })}
        class="min-h-[30px] max-h-[132px] resize-none rounded-field border-0 bg-muted px-2 py-[5px] text-13 leading-5 shadow-none focus-visible:ring-1 focus-visible:ring-primary/55 focus-visible:ring-inset aria-invalid:ring-1 aria-invalid:ring-destructive md:text-13 touch-target:min-h-11 touch-target:px-3 touch-target:py-2.5 touch-target:text-base touch-target:leading-6"
        oninput={() => invalid = false}
        onkeydown={(e) => { if (committing && e.key === 'Enter' && (mac ? e.metaKey : e.ctrlKey) && !e.isComposing) { e.preventDefault(); submit() } }} />
      {#if invalid}<p role="alert" class="mt-1 text-xs text-destructive">{$t('git.commitMessageRequired')}</p>{/if}
      <Button class="mt-1.5 h-7 w-full gap-1.5 rounded-field text-[length:var(--text-13)] touch-target:mt-2 touch-target:h-11 touch-target:text-15" disabled={busy || conflicted} onclick={submit}>
        {#if pending === 'commit' || pending === 'sync'}<CircleNotchIcon class="size-3.5 animate-spin" />{$t(pending === 'commit' ? 'git.committing' : 'git.syncing')}
        {:else if committing}<CheckIcon class="size-3.5" />{$t('git.commit')}
        {:else}<ArrowsClockwiseIcon class="size-3.5" />{$t('git.sync')}<span class="font-mono text-xs opacity-85">{[behind && `↓${behind}`, ahead && `↑${ahead}`].filter(Boolean).join(' ')}</span>{/if}
      </Button>
    {/if}
    {#if failure}
      <div role="alert" class="mt-1.5 flex items-start gap-1.5 text-destructive">
        <WarningCircleIcon class="mt-px size-3.5 shrink-0" />
        <div class="min-w-0 flex-1"><div class="text-xs font-medium">{$t(failure.title)}</div><div class="font-mono text-11 whitespace-pre-wrap break-words text-muted-foreground">{failure.detail}</div></div>
        <Button variant="ghost" size="icon-circle-xs" class="size-5 min-h-0! min-w-0!" onclick={dismiss} aria-label={$t('common.close')}><XIcon /></Button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .git-commit-box { --semantic-radius-field: var(--radius-md); }
  @media (width < 48rem), (pointer: coarse), (hover: none) {
    .git-commit-box { --semantic-radius-field: var(--radius-lg); }
  }
</style>
