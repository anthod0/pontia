<script lang="ts">
  import type { Snippet } from 'svelte';
  import CheckCircleIcon from 'phosphor-svelte/lib/CheckCircleIcon';
  import DesktopTowerIcon from 'phosphor-svelte/lib/DesktopTowerIcon';
  import LockKeyIcon from 'phosphor-svelte/lib/LockKeyIcon';
  import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon';
  import WarningCircleIcon from 'phosphor-svelte/lib/WarningCircleIcon';
  import WifiSlashIcon from 'phosphor-svelte/lib/WifiSlashIcon';
  import { Badge } from '$lib/components/ui/badge/index.js';
  import { Button } from '$lib/components/ui/button/index.js';
  import * as Card from '$lib/components/ui/card/index.js';
  import { initialRemoteDashboardState, type RemoteDashboardState } from '$lib/remoteDashboard';

  type Props = {
    handle: string;
    state?: RemoteDashboardState;
    dashboard?: Snippet;
    onRetry?: () => void;
    unavailableMessage?: string | null;
    reauthorizationUrl?: string;
  };

  let {
    handle,
    state = 'connecting',
    dashboard,
    onRetry,
    unavailableMessage,
    reauthorizationUrl,
  }: Props = $props();

  const effectiveState = $derived(
    initialRemoteDashboardState(handle) === 'invalid' ? 'invalid' : state,
  );
</script>

{#if effectiveState === 'available'}
  {@render dashboard?.()}
{:else if effectiveState === 'connecting'}
  <div class="flex min-h-96 flex-col items-center justify-center gap-3 bg-surface p-4" role="status">
    <SpinnerGapIcon class="size-6 animate-spin text-primary" aria-hidden="true" />
    <p class="text-sm text-muted-foreground">
      Connecting to <span class="font-mono text-heading">{handle}</span>…
    </p>
  </div>
{:else if effectiveState === 'unavailable'}
  <div class="flex min-h-96 flex-col items-center justify-center gap-3 bg-surface p-4 text-center">
    <WifiSlashIcon class="size-6 text-destructive" aria-hidden="true" />
    <div class="space-y-1">
      <h1 class="text-sm font-medium text-heading">Device unavailable</h1>
      <p class="text-sm text-muted-foreground">
        {#if unavailableMessage}
          {unavailableMessage}
        {:else}
          Could not connect to <span class="font-mono text-heading">{handle}</span>.
        {/if}
      </p>
    </div>
    {#if onRetry}
      <Button size="sm" variant="outline" onclick={onRetry}>Try again</Button>
    {/if}
  </div>
{:else}
  <div class="relative flex min-h-96 items-center justify-center overflow-hidden bg-surface p-4 sm:p-8">
    <div aria-hidden="true" class="pointer-events-none absolute inset-x-0 top-0 h-px bg-primary/60"></div>
    <div class="w-full max-w-xl space-y-5">
      <header class="flex items-center justify-between gap-4 px-1">
        <div class="flex items-center gap-3">
          <span class="flex size-9 items-center justify-center rounded-md border border-border bg-background text-primary">
            <DesktopTowerIcon class="size-5" />
          </span>
          <div>
            <p class="text-sm font-semibold tracking-[0.18em] text-heading">PONTIA</p>
            <p class="text-xs text-muted-foreground">Remote device</p>
          </div>
        </div>
        <Badge variant="outline" class="font-mono text-[10px] uppercase tracking-wider">Secure access</Badge>
      </header>

      <Card.Root class="bg-background">
        <Card.Header class="border-b border-border pb-4">
          <div class="flex items-start gap-3">
            <div class="mt-0.5 flex size-10 shrink-0 items-center justify-center rounded-md bg-muted">
              {#if effectiveState === 'authorization-required'}
                <LockKeyIcon class="size-5 text-warning" aria-hidden="true" />
              {:else}
                <WarningCircleIcon class="size-5 text-destructive" aria-hidden="true" />
              {/if}
            </div>
            <div class="min-w-0 space-y-1">
              {#if effectiveState === 'authorization-required'}
                <Card.Title role="heading" aria-level={1}>Authorization required</Card.Title>
                <Card.Description>Your browser needs renewed access before it can connect to this device.</Card.Description>
              {:else}
                <Card.Title role="heading" aria-level={1}>Invalid device</Card.Title>
                <Card.Description>This link does not contain a valid Pontia device handle.</Card.Description>
              {/if}
            </div>
          </div>
        </Card.Header>

        <Card.Content class="space-y-4">
          {#if effectiveState !== 'invalid'}
            <div class="rounded-md border border-border bg-muted/40 px-3 py-2.5">
              <p class="mb-1 text-[10px] font-medium uppercase tracking-wider text-muted-foreground">Device</p>
              <p class="truncate font-mono text-xs text-heading" title={handle}>{handle}</p>
            </div>
          {/if}

          {#if effectiveState === 'authorization-required'}
            <p class="text-sm text-muted-foreground">Return to Pontia to authorize this browser again. No access credentials are entered on this page.</p>
          {:else}
            <p class="text-sm text-muted-foreground">Open the device from Pontia again or check that the complete link was copied.</p>
          {/if}
        </Card.Content>

        {#if effectiveState === 'authorization-required' && reauthorizationUrl}
          <Card.Footer class="justify-end border-t border-border bg-muted/30 px-4 py-3">
            <Button href={reauthorizationUrl}>Authorize again</Button>
          </Card.Footer>
        {/if}
      </Card.Root>

      <p class="flex items-center justify-center gap-1.5 text-center text-xs text-muted-foreground">
        <CheckCircleIcon class="size-3.5 text-aqua" aria-hidden="true" />
        Device handles identify a target; authorization stays in your browser.
      </p>
    </div>
  </div>
{/if}
