<script lang="ts">
  import { t } from '$lib/i18n';
  import { Button } from '$lib/components/ui/button/index.js'
  import PontiaMascot from '../chat/PontiaMascot.svelte'
  import WorkspacesPage from '../../pages/WorkspacesPage.svelte'
  import { navigate } from '$lib/navigation'
  import { dashboardPath } from '$lib/dashboardRoutes'

  interface Props {
    canContinue: boolean
    onContinue: () => void
    routeScope?: string
  }

  let { canContinue, onContinue, routeScope }: Props = $props()
</script>

<section class="mx-auto flex min-h-[calc(100svh-5.5rem)] max-w-5xl flex-col max-md:min-h-full max-md:justify-start justify-center gap-6 md:min-h-[calc(100svh-6.5rem)]" aria-labelledby="workspace-onboarding-title">
  <div class="flex items-center gap-3 px-1">
    <PontiaMascot class="size-12 shrink-0" />
    <h1 id="workspace-onboarding-title" class="text-xl font-medium leading-snug">{$t('workspaces.setupFirst')}</h1>
  </div>

  <p class="px-1 text-sm text-muted-foreground">
    {$t('workspaces.botHint')}
    <a class="text-primary underline underline-offset-4" href={routeScope === undefined ? dashboardPath('/bot') : `${routeScope}/bot`}
      onclick={(event) => { event.preventDefault(); void navigate('/bot', undefined, { scope: routeScope }) }}>
      {$t('workspaces.openBotSetup')}
    </a>
  </p>

  <WorkspacesPage openFirstAvailableRoot={!canContinue} />

  {#if canContinue}
    <div class="flex justify-end px-1 max-md:mt-auto">
      <Button class="h-11 rounded-full px-6 max-md:w-full" onclick={onContinue}>
        {$t('workspaces.continueToChat')}
      </Button>
    </div>
  {/if}
</section>
