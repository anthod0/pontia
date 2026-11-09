<script lang="ts">
  import { onMount } from 'svelte';
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon';
  import UserIcon from 'phosphor-svelte/lib/UserIcon';
  import * as Avatar from '$lib/components/ui/avatar/index.js';
  import { Button } from '$lib/components/ui/button/index.js';
  import { Skeleton } from '$lib/components/ui/skeleton/index.js';
  import { loadRemoteAccess, type RemoteAccess } from './remoteAccessLoader.svelte';

  let profile = $state<Awaited<ReturnType<RemoteAccess['getCloudProfile']>> | null>(null);
  let accountUrl = $state<string | null>(null);
  let failed = $state(false);

  onMount(() => {
    const request = new AbortController();
    loadRemoteAccess()
      .then(async (remote) => {
        accountUrl = remote.cloudAccountUrl();
        profile = await remote.getCloudProfile(request.signal);
      })
      .catch(() => {
        if (!request.signal.aborted) failed = true;
      });
    return () => request.abort();
  });
</script>

<section class="space-y-6">
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div class="flex min-w-0 items-center gap-3">
      {#if profile}
        <Avatar.Root size="lg">
          {#if profile.avatarUrl}<Avatar.Image src={profile.avatarUrl} alt="" referrerpolicy="no-referrer" />{/if}
          <Avatar.Fallback><UserIcon class="size-5" /></Avatar.Fallback>
        </Avatar.Root>
        <div class="min-w-0">
          <p class="truncate font-medium text-heading">{profile.displayName ?? 'Pontia user'}</p>
          <p class="text-xs text-muted-foreground">Signed in to Pontia Cloud</p>
        </div>
      {:else if failed}
        <p class="text-sm text-muted-foreground">Could not load your Pontia Cloud account.</p>
      {:else}
        <Skeleton class="size-10 rounded-full" />
        <div class="space-y-1.5" role="status" aria-label="Loading account">
          <Skeleton class="h-4 w-32" />
          <Skeleton class="h-3 w-24" />
        </div>
      {/if}
    </div>
    {#if accountUrl}
      <Button variant="outline" href={accountUrl} target="_blank" rel="noopener">
        Manage account <ArrowSquareOutIcon class="size-4" />
      </Button>
    {/if}
  </div>
</section>
