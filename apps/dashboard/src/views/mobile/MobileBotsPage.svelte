<script lang="ts">
  import { useMobileHeader } from '../../shell/mobile/header'
  import { onMount, onDestroy } from 'svelte'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { t } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import * as ContextMenu from '$lib/components/ui/context-menu'
  import { botWorkStatusLabel } from '$lib/i18n/status'
  import { navigate } from '$lib/navigation'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import BotSettingsForm from '../../components/bots/BotSettingsForm.svelte'
  import BotDeleteDialog from '../../components/bots/BotDeleteDialog.svelte'
  import BotActionItems from '../../components/bots/BotActionItems.svelte'
  import PontiaMascot from '../../components/chat/PontiaMascot.svelte'
  import BotAvatar from '../../components/bots/BotAvatar.svelte'
  import { useMobileDirectory } from '../../features/bots/mobileDirectory'
  import { useDeviceRuntime } from '../../device/context'
  import type { Bot } from '../../api/types'
  import { ApiError } from '../../api/errors'
  import MobileBotMenuSheet from './bots/MobileBotMenuSheet.svelte'
  import { tapFeedback } from '$lib/mobile/haptic'
  let selected = $state<Bot>()
  let editing = $state<Bot>()
  let deleting = $state<Bot>()
  let menu = $state(false)
  let timer: ReturnType<typeof setTimeout>
  let origin: { x: number; y: number } | undefined
  let held = false
  function cancel() {
    clearTimeout(timer)
    origin = undefined
  }
  function press(event: PointerEvent, bot: Bot) {
    cancel()
    held = false
    if (event.button !== 0 || !event.isPrimary) return
    origin = { x: event.clientX, y: event.clientY }
    timer = setTimeout(() => {
      held = true
      selected = bot
      menu = true
      tapFeedback()
    }, 500)
  }
  function move(event: PointerEvent) {
    if (origin && Math.hypot(event.clientX - origin.x, event.clientY - origin.y) > 8) cancel()
  }
  async function reload(bot: Bot) {
    try {
      await directory.reload(bot.bot_id)
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
  onDestroy(cancel)
  const directory = useMobileDirectory()!
  const device = useDeviceRuntime()
  const { action } = useMobileHeader()
  onMount(() => {
    action.set(createAction)
    return () => action.set(null)
  })
  const stack = useMobileStack()!
</script>

{#snippet form()}<BotSettingsForm
    oncancel={() => stack.back()}
    onsaved={() => {
      void device.queryClient.invalidateQueries({ queryKey: ['bots'] })
      stack.back()
    }}
  />{/snippet}
{#snippet editForm()}{#if editing}<BotSettingsForm
    bot={editing}
    oncancel={() => stack.back(() => { editing = undefined })}
    onsaved={() => {
      void device.queryClient.invalidateQueries({ queryKey: ['bots'] })
      stack.back(() => { editing = undefined })
    }}
  />{/if}{/snippet}
{#snippet createAction()}
  {#if directory.error || directory.bots?.length !== 0}
    <button
      class="mobile-icon mobile-glass"
      aria-label={$t('bots.create')}
      onclick={() => stack.open({ title: $t('bots.create'), content: form, modal: true })}><PlusIcon /></button
    >
  {/if}
{/snippet}
{#each directory.bots ?? [] as bot, i (bot.bot_id)}
  {@const status = directory.summaries[i]?.status}
  {@const count = directory.summaries[i]?.open_todo_count ?? 0}
  <ContextMenu.Root>
    <ContextMenu.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          class="mobile-row min-h-[68px]!"
          onpointerdown={(e) => press(e, bot)}
          onpointerup={cancel}
          onpointercancel={cancel}
          onpointermove={move}
          onpointerleave={cancel}
          onclick={() => {
            if (!held) void navigate('/bots/' + encodeURIComponent(bot.bot_id))
            held = false
          }}
        >
          <span class="relative shrink-0">
            <BotAvatar
              class="size-11"
              shape={bot.avatar_shape}
              color={bot.avatar_color}
              working={!!status && status.kind !== 'idle'}
            />
            {#if count > 0}
              <span
                role="img"
                aria-label={$t('bots.openTodos', { count })}
                class="absolute -right-0.5 -top-0.5 size-2.5 rounded-full bg-primary ring-2 ring-surface"
              ></span>
            {/if}
          </span>
          <span class="min-w-0"
            ><strong class="block truncate">{bot.name}</strong>
            {#if status && status.kind !== 'idle'}<small class="text-primary"
                >{botWorkStatusLabel(status, $t)}</small
              >{/if}</span
          >
        </button>
      {/snippet}
    </ContextMenu.Trigger>
    <ContextMenu.Content aria-label={`${bot.name} · ${$t('bots.actions')}`} class="min-w-44">
      <BotActionItems
        edit={() => {
          editing = bot
          stack.open({ content: editForm, title: $t('bots.edit'), modal: true })
        }}
        reload={() => void reload(bot)}
        remove={() => { deleting = bot }}
        reloading={directory.reloading === bot.bot_id}
      >
        {#snippet item(action, children)}
          <ContextMenu.Item disabled={action.disabled} variant={action.destructive ? 'destructive' : 'default'} onSelect={action.select}>
            {@render children()}
          </ContextMenu.Item>
        {/snippet}
        {#snippet separator()}<ContextMenu.Separator />{/snippet}
      </BotActionItems>
    </ContextMenu.Content>
  </ContextMenu.Root>
{:else}
  {#if directory.error || !directory.bots}
    <p class="py-6 text-sm text-muted-foreground" role="status">
      {directory.error?.message ?? $t('common.loading')}
    </p>
  {:else}
    <section class="mx-auto grid max-w-md gap-5 py-4">
      <div class="flex items-center gap-3">
        <PontiaMascot class="size-10 shrink-0" />
        <h2 class="text-xl font-semibold text-heading">{$t('bots.setupFirst')}</h2>
      </div>
      <p class="text-sm text-muted-foreground">{$t('bots.setupDescription')}</p>
      <BotSettingsForm onboarding="mobile" onsaved={(bot) => void navigate('/bots/' + encodeURIComponent(bot.bot_id))} />
    </section>
  {/if}
{/each}

{#if selected}<MobileBotMenuSheet bot={selected} bind:open={menu} />{/if}
{#if deleting}<BotDeleteDialog bot={deleting} onclose={() => { deleting = undefined }} ondone={() => { deleting = undefined }} />{/if}
