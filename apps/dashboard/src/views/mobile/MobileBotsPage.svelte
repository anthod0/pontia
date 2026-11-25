<script lang="ts">
  import { useMobileHeader } from '../../shell/mobile/header'
  import { onMount, onDestroy } from 'svelte'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import { t } from '$lib/i18n'
  import { botWorkStatusLabel } from '$lib/i18n/status'
  import { navigate } from '$lib/navigation'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  import BotSettingsForm from '../../components/bots/BotSettingsForm.svelte'
  import PontiaMascot from '../../components/chat/PontiaMascot.svelte'
  import BotAvatar from '../../components/bots/BotAvatar.svelte'
  import { useMobileDirectory } from '../../features/bots/mobileDirectory'
  import { useDeviceRuntime } from '../../device/context'
  import type { Bot } from '../../api/types'
  import MobileBotMenuSheet from './bots/MobileBotMenuSheet.svelte'
  import { tapFeedback } from '$lib/mobile/haptic'
  let selected = $state<Bot>()
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
  <button
    class="mobile-row min-h-[68px]!"
    onpointerdown={(e) => press(e, bot)}
    onpointerup={cancel}
    onpointercancel={cancel}
    onpointermove={move}
    onpointerleave={cancel}
    oncontextmenu={(e) => {
      e.preventDefault()
      cancel()
      selected = bot
      menu = true
      held = true
    }}
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
