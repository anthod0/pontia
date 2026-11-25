<script lang="ts">
  import { createQuery } from '@tanstack/svelte-query'
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { dashboardPath } from '$lib/dashboardRoutes'
  import { Button } from '$lib/components/ui/button/index.js'
  import { useDeviceRuntime } from '../device/context'
  import BotSettingsForm from '../components/bots/BotSettingsForm.svelte'
  import PontiaMascot from '../components/chat/PontiaMascot.svelte'
  import BotChatSkeleton from '../components/bots/BotChatSkeleton.svelte'
  import MobileBotChatSkeleton from '../views/mobile/bots/MobileBotChatSkeleton.svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'

  let { routeScope }: { routeScope?: string } = $props()
  const device = useDeviceRuntime()
  const bots = createQuery(
    () => ({ queryKey: ['bots'], queryFn: ({ signal }) => device.api.listBots(signal) }),
    () => device.queryClient,
  )

  $effect(() => {
    const first = bots.data?.[0]
    if (first) void navigate(`/bots/${encodeURIComponent(first.bot_id)}`, undefined, { scope: routeScope, replaceState: true })
  })
</script>

{#if !bots.isError && (bots.isPending || bots.data?.length)}
  <!-- The page opens the first Bot's chat, so it loads in that layout. -->
  {#if isMobileShell()}<MobileBotChatSkeleton />{:else}<BotChatSkeleton />{/if}
{:else}
<section class="mx-auto flex min-h-[calc(100svh-5.5rem)] max-w-md flex-col justify-center gap-6 py-8 md:min-h-[calc(100svh-6.5rem)]">
  {#if bots.isError}
    <p role="alert" class="text-sm text-destructive">{bots.error.message}</p>
    <Button variant="outline" onclick={() => void bots.refetch()}>{$t('common.retry')}</Button>
  {:else}
    <div class="flex items-center gap-3">
      <PontiaMascot class="size-12 shrink-0" />
      <h1 class="text-xl font-medium leading-snug">{$t('bots.setupFirst')}</h1>
    </div>
    <p class="text-sm text-muted-foreground">{$t('bots.setupDescription')}</p>
    <BotSettingsForm onboarding="desktop" onsaved={(bot) => {
      device.queryClient.setQueryData(['bots'], [bot])
    }} />
    <p class="text-sm text-muted-foreground">
      {$t('bots.workspaceHint')}
      <a class="text-primary underline underline-offset-4" href={routeScope === undefined ? dashboardPath('/') : routeScope || '/'}
        onclick={(event) => { event.preventDefault(); void navigate('/', undefined, { scope: routeScope }) }}>
        {$t('bots.openWorkspaceSetup')}
      </a>
    </p>
  {/if}
</section>
{/if}
