<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { navigate, routeParam } from '$lib/navigation'
  import { forgetLastPage } from '$lib/lastPage'
  import { sessionChatTitle } from '$lib/session-chat/sessionChat'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import { useDeviceRuntime } from '../../device/context'
  import type { Bot } from '../../api/types'
  import { removeCachedBotTimeline } from '$lib/botTimelineCache'

  // ondone also closes the dialogs that opened this one, after deletion or when leaving for a helper.
  let { bot, onclose, ondone }: { bot: Bot; onclose: () => void; ondone: () => void } = $props()
  const device = useDeviceRuntime()
  const blockers = createQuery(
    () => ({
      queryKey: ['bot-deletion-blockers', bot.bot_id],
      queryFn: ({ signal }) => device.api.getBotDeletionBlockers(bot.bot_id, signal),
      staleTime: 0,
    }),
    () => device.queryClient,
  )
  const bots = createQuery(
    () => ({ queryKey: ['bots'], queryFn: ({ signal }) => device.api.listBots(signal) }),
    () => device.queryClient,
  )
  const blocked = $derived(!blockers.data || blockers.data.running || blockers.data.helpers.length > 0)
  const last = $derived(bots.data?.length === 1)
  let deleting = $state(false)
  let error = $state('')

  async function remove() {
    // Removing the Bot from the cache unmounts the dialogs that render this one, so read props first.
    const id = bot.bot_id
    const done = ondone
    const viewing = routeParam('bots') === id
    deleting = true
    error = ''
    try {
      await device.api.deleteBot(id)
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure)
      deleting = false
      void blockers.refetch()
      return
    }
    device.queryClient.setQueryData<Bot[]>(['bots'], (current) => current?.filter((item) => item.bot_id !== id))
    device.queryClient.removeQueries({ queryKey: ['bot-status', id] })
    forgetLastPage(device.id, { kind: 'bot', id })
    void device.queryClient.invalidateQueries({ queryKey: ['bots'] })
    done()
    // Leaving the deleted Bot's chat opens the first remaining Bot, or the first-Bot setup when none is left.
    if (viewing) await navigate('/bot', undefined, { replaceState: true })
    // After navigation the deleted Bot's chat has closed and cannot write its cache again.
    await removeCachedBotTimeline(device.id, id)
  }

  function openHelper(sessionId: string) {
    ondone()
    void navigate(`/chat/${encodeURIComponent(sessionId)}`)
  }
</script>

<AlertDialog.Root open onOpenChange={(open) => { if (!open && !deleting) onclose() }}>
  <AlertDialog.Content class="max-h-[calc(100dvh-2rem)] overflow-y-auto">
    <AlertDialog.Header>
      <AlertDialog.Title class="break-words">{$t('bots.deleteTitle', { name: bot.name })}</AlertDialog.Title>
      <AlertDialog.Description class="grid gap-2 text-left">
        <span>{$t('bots.deleteDescription')}</span>
        <span>{$t('bots.deleteKeeps')}</span>
        {#if last}<span>{$t('bots.deleteLast')}</span>{/if}
      </AlertDialog.Description>
    </AlertDialog.Header>
    {#if blockers.isError}
      <p role="alert" class="text-sm text-destructive">{blockers.error.message}</p>
    {:else if blockers.data?.running}
      <p role="alert" class="text-sm text-destructive">{$t('bots.deleteRunning')}</p>
    {/if}
    {#if blockers.data?.helpers.length}
      <div class="grid gap-2" role="alert">
        <p class="text-sm text-destructive">{$t('bots.deleteHelpersOpen')}</p>
        <ul class="grid gap-1" aria-label={$t('bots.openHelpers')}>
          {#each blockers.data.helpers as helper (helper.session_id)}
            <li>
              <Button variant="ghost" size="sm" class="h-auto w-full justify-start py-1.5 text-left font-normal whitespace-normal [overflow-wrap:anywhere]" onclick={() => openHelper(helper.session_id)}>
                {sessionChatTitle(helper, $t)}
              </Button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}
    {#if error}<p role="alert" class="text-sm text-destructive break-words">{error}</p>{/if}
    <AlertDialog.Footer>
      <AlertDialog.Cancel disabled={deleting}>{$t('common.cancel')}</AlertDialog.Cancel>
      <Button variant="destructive" disabled={blocked || deleting} onclick={() => void remove()}>
        {$t(deleting ? 'bots.deleting' : 'bots.delete')}
      </Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
