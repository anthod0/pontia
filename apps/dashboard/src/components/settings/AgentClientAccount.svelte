<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import CircleNotchIcon from 'phosphor-svelte/lib/CircleNotchIcon'
  import CopyIcon from 'phosphor-svelte/lib/CopyIcon'
  import CheckIcon from 'phosphor-svelte/lib/CheckIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Item from '$lib/components/ui/item/index.js'
  import { createCopyFeedback } from '$lib/copyFeedback.svelte'
  import { useDeviceRuntime } from '../../device/context'
  import { cn } from '$lib/utils'
  import type { AgentClientAuthStatus, AgentClientView } from '../../api/types'

  let { client, mobile = false }: { client: AgentClientView; mobile?: boolean } = $props()
  const device = useDeviceRuntime()
  let auth = $state<AgentClientAuthStatus | null>(null)
  let loading = $state(false)
  let error = $state('')
  const copyFeedback = createCopyFeedback()

  const guidance = $derived(client.install?.auth)
  const command = $derived(guidance?.headless_login_command ?? guidance?.login_command)
  const title = $derived(cn('text-heading', mobile && 'text-15'))
  const description = $derived(cn('line-clamp-none break-words text-xs [overflow-wrap:anywhere]', mobile && 'text-13 leading-[18px]'))

  onMount(() => {
    if (!client.auth_supported) return
    loading = true
    const controller = new AbortController()
    device.api
      .getAgentClientAuth(client, { signal: controller.signal })
      .then((value) => (auth = value))
      .catch((cause) => {
        if (!controller.signal.aborted) error = cause instanceof Error ? cause.message : String(cause)
      })
      .finally(() => (loading = false))
    return () => controller.abort()
  })

  async function copyCommand() {
    await copyFeedback.copy(command)
  }
</script>

<Item.Root class="min-h-14 flex-nowrap items-start gap-3 rounded-none px-0 py-2.5">
  <Item.Content class="min-w-0 gap-1">
    <Item.Title class={title}>{$t('clients.account.title')}</Item.Title>
    <Item.Description class={description}>
      {#if loading}
        <span class="inline-flex items-center gap-1.5"><CircleNotchIcon class="size-3 animate-spin" />{$t('clients.account.checking')}</span>
      {:else if auth?.state === 'authenticated'}
        {$t('clients.account.authenticated', { account: auth.account?.label ?? auth.account?.plan ?? client.name })}
      {:else if auth?.state === 'unauthenticated' || auth?.state === 'expired'}
        <span class="text-warning">{$t('clients.account.notSignedIn')}</span>
      {:else if guidance?.status === 'configured'}
        {$t('clients.account.configured')}
      {:else if command}
        {$t('clients.account.terminal')}
      {:else if error || auth?.detail}
        <span class="text-warning">{auth?.detail ?? error}</span>
      {:else}
        {$t('clients.account.unknown')}
      {/if}
    </Item.Description>
    {#if (!client.auth_supported || auth?.methods.length === 0) && guidance?.status !== 'configured' && command}
      <div class="mt-1 flex min-w-0 items-center gap-1 rounded-item bg-muted px-2 py-1">
        <code class="min-w-0 flex-1 truncate font-mono text-xs text-foreground" title={command}>{command}</code>
        <Button variant="ghost" size="icon-sm" class="-my-1 -mr-1" aria-label={$t(copyFeedback.confirmed ? 'common.copied' : 'clients.account.copyCommand')} onclick={copyCommand}>{#if copyFeedback.confirmed}<CheckIcon />{:else}<CopyIcon />{/if}</Button>
      </div>
      {#if guidance?.env_vars.length}<p class="mt-1">{$t('clients.account.env', { vars: guidance.env_vars.join('、') })}</p>{/if}
    {/if}
  </Item.Content>
  {#if guidance?.docs_url}
    <Item.Actions>
      <Button variant="ghost" size="sm" href={guidance.docs_url} target="_blank" rel="noreferrer">{$t('clients.account.docs')}<ArrowSquareOutIcon data-icon="inline-end" /></Button>
    </Item.Actions>
  {/if}
</Item.Root>
