<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import { clientStatus, createAgentClients } from '../../../features/settings/agentClients.svelte'
  import ClientSettingsGroups from '../../../components/settings/ClientSettingsGroups.svelte'
  import ClientStatus from '../../../components/settings/ClientStatus.svelte'
  const settings = createAgentClients()
</script>

<section class="mx-auto max-w-4xl">
  <div class="mb-7 flex items-center justify-between gap-4">
    <p class="text-sm text-muted-foreground">{$t('clients.pageDescription')}</p>
    <Button
      variant="ghost"
      class="-mr-2.5 shrink-0"
      disabled={settings.refreshing || settings.loading}
      onclick={settings.refresh}
      ><ArrowClockwiseIcon data-icon="inline-start" class={settings.refreshing ? 'animate-spin' : undefined} />{$t(
        'common.refresh',
      )}</Button
    >
  </div>

  {#if settings.error}
    <div role="alert" class="mb-6 flex items-center gap-2 text-sm text-destructive">
      <WarningCircleIcon class="size-4 shrink-0" />
      <span class="min-w-0 flex-1">{settings.error}</span>
      {#if settings.loadFailed}<Button variant="ghost" size="sm" class="-mr-2.5" onclick={settings.refresh}
          >{$t('common.retry')}</Button
        >{/if}
    </div>
  {/if}

  {#if settings.loading}
    <div role="status" aria-label={$t('clients.loading')} class="grid items-start gap-10 lg:grid-cols-[13rem_minmax(0,1fr)]">
      <div class="space-y-0.5">
        {#each Array(5) as _, index (index)}
          <div class="flex min-h-13 items-center gap-2.5 px-3 py-2">
            <Skeleton class="size-5" />
            <div class="space-y-1.5"><Skeleton class="h-3.5 w-28" /><Skeleton class="h-2.5 w-16" /></div>
          </div>
        {/each}
      </div>
      <div>
        <div class="mb-1 flex min-h-9 items-center gap-3"><Skeleton class="size-7" /><Skeleton class="h-4.5 w-30" /></div>
        {#each [[60, 110], [40, 40, 80]] as widths, group (group)}
          <div class="mt-7 divide-y divide-foreground/6 dark:divide-foreground/7">
            {#each widths as width, index (index)}
              <div class="flex min-h-14 items-center gap-3 py-2.5">
                <div class="flex-1 space-y-1.5">
                  <Skeleton class="h-3.5" style="width: {width}px" /><Skeleton class="h-2.5" style="width: {width + 120}px" />
                </div>
                <Skeleton class="h-[18px] w-8 rounded-action" />
              </div>
            {/each}
          </div>
        {/each}
      </div>
    </div>
  {:else if !settings.loadFailed}
    <div class="grid items-start gap-10 lg:grid-cols-[13rem_minmax(0,1fr)]">
      <nav aria-label={$t('clients.title')}>
        {#each settings.groups as [detection, list], index (detection)}
          <div class:mt-4={index > 0}>
            <p class="px-3 pb-1.5 text-xs text-muted-foreground">
              {$t(`clients.group.${detection}`, { count: list.length })}
            </p>
            <div class="space-y-0.5">
              {#each list as client (client.id)}
                <button
                  type="button"
                  class="flex min-h-13 w-full items-center gap-2.5 rounded-item px-3 py-2 text-left transition-colors hover:bg-muted/60 aria-[current=true]:bg-muted"
                  aria-current={settings.selected?.id === client.id ? 'true' : undefined}
                  onclick={() => settings.select(client)}
                >
                  <ClientIcon
                    clientType={client.client_type}
                    acpAgentId={client.client_key}
                    class="size-5 shrink-0 {detection === 'not_detected' ? 'opacity-50' : ''}"
                  />
                  <span class="min-w-0 flex-1 space-y-0.5">
                    <span class="block truncate text-sm font-medium text-heading">{client.name}</span>
                    <ClientStatus status={clientStatus(client, settings.operationFor(client), settings.installOperationFor(client))} />
                  </span>
                </button>
              {/each}
            </div>
          </div>
        {:else}
          <p class="px-3 py-2 text-sm text-muted-foreground">{$t('clients.empty')}</p>
        {/each}
      </nav>

      {#if settings.selected}
        <div class="min-w-0">
          <div class="mb-1 flex min-h-9 items-center gap-3">
            <ClientIcon
              clientType={settings.selected.client_type}
              acpAgentId={settings.selected.client_key}
              class="size-7 shrink-0"
            />
            <h3 class="truncate text-lg font-semibold text-heading">{settings.selected.name}</h3>
          </div>
          {#key settings.selected.id}<ClientSettingsGroups {settings} client={settings.selected} />{/key}
        </div>
      {/if}
    </div>
  {/if}
</section>
