<script lang="ts">
  import { useMobileHeader } from '../../../shell/mobile/header'
  import { t } from '$lib/i18n'
  import ArrowClockwiseIcon from 'phosphor-svelte/lib/ArrowClockwiseIcon'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon'
  import ClientIcon from '../../../components/chat/ClientIcon.svelte'
  import { Button } from '$lib/components/ui/button/index.js'
  import { Skeleton } from '$lib/components/ui/skeleton/index.js'
  import * as Item from '$lib/components/ui/item/index.js'
  import { clientStatus, createAgentClients } from '../../../features/settings/agentClients.svelte'
  import ClientSettingsGroups from '../../../components/settings/ClientSettingsGroups.svelte'
  import ClientStatus from '../../../components/settings/ClientStatus.svelte'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  const settings = createAgentClients()
  const stack = useMobileStack()!
  const { action } = useMobileHeader()
  $effect(() => {
    action.set(refresh)
    return () => action.set(null)
  })
</script>

{#if settings.error && !settings.loadFailed}
  <p role="alert" class="mb-4 flex items-center gap-2 text-13 text-destructive">
    <WarningCircleIcon class="size-4 shrink-0" />{settings.error}
  </p>
{/if}

{#if settings.loading}
  <div role="status" aria-label={$t('clients.loading')}>
    {#each Array(4) as _, index (index)}
      <div class="flex min-h-14 items-center gap-3 py-2.5">
        <Skeleton class="size-5" />
        <div class="space-y-1.5"><Skeleton class="h-3.5 w-30" /><Skeleton class="h-2.5 w-20" /></div>
      </div>
    {/each}
  </div>
{:else if settings.loadFailed}
  <div role="alert" class="flex items-center gap-2 text-13 text-destructive">
    <WarningCircleIcon class="size-4 shrink-0" />
    <span class="min-w-0 flex-1">{settings.error}</span>
    <Button variant="ghost" class="-mr-3 h-9 px-3 text-sm" onclick={settings.refresh}>{$t('common.retry')}</Button>
  </div>
{:else}
  <p class="mb-6 text-13 leading-[1.45] text-muted-foreground">{$t('clients.pageDescription')}</p>
  <nav aria-label={$t('clients.title')}>
    {#each settings.groups as [detection, list] (detection)}
      <section class="mb-7">
        <h2 class="mb-0.5 text-13 leading-[18px] font-medium text-muted-foreground">
          {$t(`clients.group.${detection}`, { count: list.length })}
        </h2>
        <Item.Group class="gap-0">
          {#each list as client, index (client.id)}
            {#if index > 0}<Item.Separator
                class="my-0 ml-8 bg-foreground/6 data-[orientation=horizontal]:w-[calc(100%-2rem)] dark:bg-foreground/7"
              />{/if}
            <Item.Root class="-mx-3 min-h-14 w-auto flex-nowrap gap-3 px-3 py-2.5 active:bg-muted">
              {#snippet child({ props })}
                <button
                  {...props}
                  type="button"
                  onclick={() => {
                    settings.select(client)
                    stack.open({ title: client.name, content: detail })
                  }}
                >
                  <Item.Media>
                    <ClientIcon
                      clientType={client.client_type}
                      clientKey={client.client_key}
                      class="size-5 {detection === 'not_detected' ? 'opacity-50' : ''}"
                    />
                  </Item.Media>
                  <Item.Content class="min-w-0 gap-0.5 text-left">
                    <Item.Title class="text-15 text-heading">{client.name}</Item.Title>
                    <ClientStatus class="text-13" status={clientStatus(client, settings.operationFor(client), settings.installOperationFor(client))} />
                  </Item.Content>
                  <CaretRightIcon class="size-4 shrink-0 text-muted-foreground" />
                </button>
              {/snippet}
            </Item.Root>
          {/each}
        </Item.Group>
      </section>
    {:else}
      <p class="py-2 text-sm text-muted-foreground">{$t('clients.empty')}</p>
    {/each}
  </nav>
{/if}
{#snippet refresh()}<Button
    size="icon"
    variant="ghost"
    aria-label={$t('common.refresh')}
    disabled={settings.refreshing || settings.loading}
    onclick={settings.refresh}
    ><ArrowClockwiseIcon class={settings.refreshing ? 'animate-spin' : undefined} /></Button
  >{/snippet}
{#snippet detail()}{#if settings.error}<p role="alert" class="mb-3 text-13 text-destructive">
      {settings.error}
    </p>{/if}{#if settings.selected}<ClientSettingsGroups
      {settings}
      client={settings.selected}
      mobile
    />{/if}{/snippet}
