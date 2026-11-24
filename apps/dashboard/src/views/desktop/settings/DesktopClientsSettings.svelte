<script lang="ts">
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ProhibitIcon from 'phosphor-svelte/lib/ProhibitIcon'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import { createAgentClients } from '../../../features/settings/agentClients.svelte'
  import ClientDetail from '../../../components/settings/ClientDetail.svelte'
  const settings = createAgentClients()
</script>

<section class="space-y-6">
  <div class="flex flex-wrap justify-end gap-2">
    <Button
      variant="outline"
      disabled={settings.busy || settings.loading}
      onclick={() => settings.perform(settings.refresh)}
      ><ArrowClockwiseIcon data-icon="inline-start" />{$t('common.refresh')}</Button
    >
  </div>

  {#if settings.error}
    <Alert.Root variant="destructive">
      <WarningCircleIcon class="size-4" />
      <Alert.Description>{settings.error}</Alert.Description>
    </Alert.Root>
  {/if}

  {#if settings.loading}
    <div role="status" aria-label={$t('clients.loading')} class="grid gap-4 lg:grid-cols-[11rem_minmax(0,1fr)]">
      <Skeleton class="h-64 w-full" /><Skeleton class="h-96 w-full" />
    </div>
  {:else}
    <div class="grid items-start gap-4 lg:grid-cols-[11rem_minmax(0,1fr)]">
      <nav aria-label={$t('clients.title')} class="space-y-0.5">
        {#each settings.clients as client (client.id)}
          <button
            type="button"
            class="flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-left text-sm transition-colors hover:bg-muted disabled:pointer-events-none disabled:opacity-60 {settings
              .selected?.id === client.id
              ? 'bg-muted font-medium text-heading'
              : client.detection.status !== 'not_detected' && client.enabled
                ? 'text-foreground'
                : 'text-muted-foreground'}"
            aria-current={settings.selected?.id === client.id ? 'true' : undefined}
            disabled={settings.saving || settings.startingSetup}
            onclick={() => settings.select(client)}
          >
            <ClientIcon
              clientType={client.client_type}
              acpAgentId={client.client_key}
              class="size-4 {client.detection.status !== 'not_detected' && client.enabled
                ? 'text-foreground'
                : 'opacity-50'}"
            />
            <span class="min-w-0 flex-1">
              <span class="block truncate">{client.name}</span>
              <span class="block text-xs font-normal text-muted-foreground"
                >{$t(`clients.detection.${client.detection.status}`)}</span
              >
            </span>
            {#if !client.enabled}<ProhibitIcon
                class="size-3.5 shrink-0 text-muted-foreground"
                aria-label={$t('common.disabled')}
              />{/if}
          </button>
        {:else}
          <p class="px-3 py-2 text-sm text-muted-foreground">{$t('clients.empty')}</p>
        {/each}
      </nav>

      {#if settings.selected}
        <ClientDetail {settings} />
      {:else}
        <p class="py-2 text-sm text-muted-foreground">{$t('clients.selectClient')}</p>
      {/if}
    </div>
  {/if}
</section>
