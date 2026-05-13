<script lang="ts">
  import { onMount } from 'svelte';
  import EmptyState from '../common/EmptyState.svelte';
  import ErrorBanner from '../common/ErrorBanner.svelte';
  import LoadingState from '../common/LoadingState.svelte';
  import { agentProfiles, agentProfilesError, agentProfilesLoading, loadAgentProfiles } from '../../stores/agentProfiles';

  onMount(() => {
    void loadAgentProfiles();
  });
</script>

<section class="panel">
  <div class="panel-heading">
    <h2>Agent profiles</h2>
  </div>
  <ErrorBanner message={$agentProfilesError} />
  {#if $agentProfilesLoading}
    <LoadingState message="Loading agent profiles..." />
  {:else if !$agentProfiles.length}
    <EmptyState message="No agent profiles." />
  {:else}
    <div class="list">
      {#each $agentProfiles as profile (profile.profile_id)}
        <div class="item passive">
          <strong>{profile.name}</strong>
          <span>{profile.profile_id}@{profile.version} · {profile.session_reuse_policy}</span>
          <small class="muted">{profile.description ?? 'No description'}</small>
          <small class="muted">Clients: {profile.supported_client_types.join(', ') || 'any'}</small>
        </div>
      {/each}
    </div>
  {/if}
</section>
