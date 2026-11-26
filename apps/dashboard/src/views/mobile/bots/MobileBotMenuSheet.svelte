<script lang="ts">
  import type { Bot } from '../../../api/types'
  import { useDeviceRuntime } from '../../../device/context'
  import { ApiError } from '../../../api/errors'
  import { t } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import BotSettingsForm from '../../../components/bots/BotSettingsForm.svelte'
  import BotDeleteDialog from '../../../components/bots/BotDeleteDialog.svelte'
  import BotAvatar from '../../../components/bots/BotAvatar.svelte'
  import MobileBotSheet from './MobileBotSheet.svelte'
  let {
    bot,
    open = $bindable(false),
    onselect,
    onsaved,
  }: {
    bot: Bot
    open: boolean
    onselect?: (tab: 'todos' | 'search' | 'files' | 'git') => void
    onsaved?: () => void
  } = $props()
  const device = useDeviceRuntime()
  const stack = useMobileStack()!
  let deleting = $state(false)
  async function reload() {
    try {
      await device.api.reloadBot(bot.bot_id)
      toast.success($t('bots.reloadReady'))
    } catch (error) {
      toast.error(
        error instanceof ApiError && error.code === 'state_conflict'
          ? $t('bots.reloadBusy')
          : error instanceof Error
            ? error.message
            : String(error),
        { duration: 6000 },
      )
    }
  }
</script>

{#snippet edit()}<BotSettingsForm {bot} oncancel={() => stack.back()} onsaved={() => stack.back(onsaved)} />
  <button class="mobile-row text-destructive" onclick={() => stack.back(() => (deleting = true))}>{$t('bots.delete')}</button>{/snippet}
<MobileBotSheet bind:open title={bot.name}>
  <p class="flex items-center gap-2 pb-3 text-muted-foreground">
    <BotAvatar shape={bot.avatar_shape} color={bot.avatar_color} class="size-9" />{bot.client_key} · {bot.model}
  </p>
  {#if onselect}
    {#each [['todos', 'bots.todos'], ['search', 'bots.search'], ['files', 'files.title'], ['git', 'git.title']] as const as [key, label]}
      <button class="mobile-row" onclick={() => stack.back(() => onselect?.(key))}>{$t(label)}</button>
    {/each}
  {/if}
  <button
    class="mobile-row"
    onclick={() => stack.back(() => stack.open({ content: edit, title: $t('bots.edit'), modal: true }))}
    >{$t('bots.edit')}</button
  >
  <button class="mobile-row" onclick={() => stack.back(reload)}>{$t('bots.reload')}</button>
  <button class="mobile-row text-destructive" onclick={() => stack.back(() => (deleting = true))}
    >{$t('bots.delete')}</button
  >
</MobileBotSheet>
{#if deleting}<BotDeleteDialog {bot} onclose={() => (deleting = false)} ondone={() => (deleting = false)} />{/if}
