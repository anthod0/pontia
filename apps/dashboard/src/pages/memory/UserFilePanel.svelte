<script lang="ts">
  import { t, currentLocale } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import { createCopyFeedback } from '$lib/copyFeedback.svelte'
  import { ApiError } from '../../api/errors'
  import { useDeviceRuntime } from '../../device/context'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { Textarea } from '$lib/components/ui/textarea/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js'
  import MessageResponse from '$lib/components/ai-elements/message/message-response.svelte'
  import UserCircleIcon from 'phosphor-svelte/lib/UserCircleIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import PencilSimpleIcon from 'phosphor-svelte/lib/PencilSimpleIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import { formatRelativeTime } from '$lib/format'

  import type { Snippet } from 'svelte'
  let { openEditor, closeEditor }: { openEditor?: (editor: Snippet) => void; closeEditor?: () => void } = $props()
  const device = useDeviceRuntime()
  const user = device.memoryQueries.createUserFileQuery()
  const replace = device.memoryQueries.createReplaceUserFileMutation()
  let mode = $state<'rendered' | 'source'>('rendered')
  // While editing, `base` is the text the draft started from.
  let editing = $state<{ base: string; draft: string } | null>(null)
  let saveError = $state<{ message: string; conflict: boolean } | null>(null)
  const copyFeedback = createCopyFeedback()
  const length = $derived(user.data ? [...user.data.text].length : 0)
  const draftLength = $derived(editing ? [...editing.draft].length : 0)
  const overLimit = $derived(!!user.data && draftLength > user.data.character_limit)

  async function copy(): Promise<void> {
    await copyFeedback.copy(user.data?.text)
  }

  function edit(text: string): void {
    editing = { base: text, draft: text }
    saveError = null
  }

  async function save(): Promise<void> {
    if (!editing) return
    try {
      await replace.mutateAsync({ text: editing.draft, previous_text: editing.base })
      closeEditor?.()
      editing = null
      saveError = null
      toast.success($t('memory.userSaved'))
    } catch (error) {
      const conflict = error instanceof ApiError && error.code === 'state_conflict'
      saveError = { message: conflict ? $t('memory.userConflict') : (error as Error).message, conflict }
      if (conflict) void user.refetch()
    }
  }

  function beginEdit(text: string) {
    edit(text)
    openEditor?.(editor)
  }
  function startOver(): void {
    if (user.data) edit(user.data.text)
  }
</script>

{#snippet actions()}        <Button size="sm" variant="ghost" disabled={replace.isPending} onclick={() => { closeEditor?.(); editing = null; saveError = null }}>{$t('common.cancel')}</Button>
        <Button size="sm" type="submit" disabled={replace.isPending || overLimit || !editing || editing.draft === editing.base}>{replace.isPending ? $t('common.saving') : $t('common.save')}</Button>
{/snippet}
{#snippet editor()}
  {#if editing && user.data}
    <form class={openEditor ? 'flex min-h-full flex-col' : 'contents'} onsubmit={(event) => { event.preventDefault(); void save() }}>
      <div class="flex flex-wrap items-center gap-x-3 gap-y-2 border-b px-4 py-2">
        <div class="flex min-w-0 flex-1 items-center gap-2">
          <span class="font-mono text-sm font-medium text-heading">USER.md</span>
          <span class={['text-xs', overLimit ? 'text-destructive' : 'text-muted-foreground']}>
            {$t('memory.userCharacters', { count: draftLength.toLocaleString($currentLocale), limit: user.data.character_limit.toLocaleString($currentLocale) })}
          </span>
        </div>
        {#if !openEditor}{@render actions()}{/if}
      </div>
      {#if saveError}
        <Alert.Root variant="destructive" class="mx-4 mt-4 w-auto">
          <WarningCircleIcon class="size-4" />
          <Alert.Description>
            <p>{saveError.message}</p>
            {#if saveError.conflict}<Button size="sm" variant="outline" class="mt-2" disabled={user.isFetching} onclick={startOver}>{$t('memory.userStartOver')}</Button>{/if}
          </Alert.Description>
        </Alert.Root>
      {/if}
      <div class="px-4 py-4">
        <Textarea
          bind:value={editing.draft}
          aria-label={$t('memory.userEditor')}
          aria-invalid={overLimit}
          spellcheck={false}
          disabled={replace.isPending}
          class="max-h-[34rem] min-h-72 font-mono text-xs leading-relaxed md:text-xs"
        />
      </div>
      {#if openEditor}<p class="px-4 py-2 text-xs text-muted-foreground">{$t('memory.userEditHint')}</p><div class="sticky bottom-0 mt-auto flex justify-end gap-2 border-t bg-surface py-3">{@render actions()}</div>{/if}
    </form>
    {#if !openEditor}<p class="border-t px-4 py-2 text-xs text-muted-foreground">{$t('memory.userEditHint')}</p>{/if}
  {/if}
{/snippet}
<div class="overflow-hidden rounded-item border bg-background text-sm shadow-sm">
  {#if editing && !openEditor}{@render editor()}
  {:else if user.isError}
    <Alert.Root variant="destructive" class="m-4 w-auto"><WarningCircleIcon class="size-4" /><Alert.Description>{user.error.message}</Alert.Description></Alert.Root>
  {:else if user.isPending}
    <div class="space-y-3 p-4" role="status" aria-label={$t('common.loading')}><Skeleton class="h-6 w-48" /><Skeleton class="h-48 w-full" /></div>
  {:else if !user.data.text.trim()}
    <Empty.Root class="min-h-72">
      <Empty.Header>
        <Empty.Media variant="icon"><UserCircleIcon /></Empty.Media>
        <Empty.Title>{$t('memory.userEmptyTitle')}</Empty.Title>
        <Empty.Description>{$t('memory.userEmptyDescription')}</Empty.Description>
      </Empty.Header>
      <Empty.Content>
        <Button size="sm" variant="outline" onclick={() => beginEdit(user.data.text)}><PencilSimpleIcon data-icon="inline-start" />{$t('memory.userWrite')}</Button>
      </Empty.Content>
    </Empty.Root>
  {:else}
    <div class="flex flex-wrap items-center gap-x-3 gap-y-2 border-b px-4 py-2">
      <div class="flex min-w-0 flex-1 items-center gap-2">
        <span class="font-mono text-sm font-medium text-heading">USER.md</span>
        <span class="text-xs text-muted-foreground">
          {$t('memory.userCharacters', { count: length.toLocaleString($currentLocale), limit: user.data.character_limit.toLocaleString($currentLocale) })}
          {#if user.data.updated_at}· {$t('memory.updatedRelative', { time: formatRelativeTime(user.data.updated_at, $currentLocale) })}{/if}
        </span>
      </div>
      <ToggleGroup.Root type="single" size="sm" variant="outline" value={mode} onValueChange={(value) => { if (value) mode = value as typeof mode }} aria-label={$t('memory.userView')}>
        <ToggleGroup.Item value="rendered" class="px-2.5">{$t('memory.userRendered')}</ToggleGroup.Item>
        <ToggleGroup.Item value="source" class="px-2.5">{$t('memory.userSource')}</ToggleGroup.Item>
      </ToggleGroup.Root>
      <Button size="icon-sm" variant="ghost" aria-label={$t(copyFeedback.confirmed ? 'common.copied' : 'common.copy')} title={$t(copyFeedback.confirmed ? 'common.copied' : 'common.copy')} onclick={() => void copy()}>{#if copyFeedback.confirmed}<CheckIcon class="size-4" />{:else}<CopyIcon class="size-4" />{/if}</Button>
      <Button size="sm" variant="outline" onclick={() => beginEdit(user.data.text)}><PencilSimpleIcon data-icon="inline-start" />{$t('common.edit')}</Button>
    </div>
    <div class="max-h-[34rem] overflow-auto px-5 py-4">
      {#if mode === 'rendered'}
        <MessageResponse markdown content={user.data.text} />
      {:else}
        <pre class="font-mono text-xs leading-relaxed whitespace-pre-wrap text-foreground">{user.data.text}</pre>
      {/if}
    </div>
    <p class="border-t px-4 py-2 text-xs text-muted-foreground">{$t('memory.userHint')}</p>
  {/if}
</div>
