<script lang="ts">
  import { onMount } from 'svelte';
  import { createSession } from '../../api/client';
  import { loadSessions } from '../../stores/sessions';
  import { selectSession } from '../../stores/selection';
  import { setStatus } from '../../stores/ui';
  import {
    agentProfiles,
    clientTypeOptionsForProfile,
    defaultHandleForProfile,
    loadAgentProfiles,
    metadataForProfile,
    selectClientTypeForProfile,
  } from '../../stores/agentProfiles';
  import WorkspaceSelector from '../workspaces/WorkspaceSelector.svelte';

  let profileId = '';
  let clientType = 'claude_code';
  let workspaceId = '';
  let handle = '';
  let role = '';
  let description = '';
  let initialTask = '';
  let creating = false;

  $: selectedProfile = $agentProfiles.find((profile) => profile.profile_id === profileId) ?? null;
  $: clientTypeOptions = clientTypeOptionsForProfile(selectedProfile);

  onMount(() => {
    void loadAgentProfiles();
  });

  function applyProfileDefaults() {
    clientType = selectClientTypeForProfile(clientType, selectedProfile);
    if (!handle.trim()) handle = defaultHandleForProfile(selectedProfile);
    if (!role.trim()) role = selectedProfile?.default_session_role ?? '';
    if (!description.trim()) description = selectedProfile?.default_session_description ?? '';
  }

  async function submit() {
    creating = true;
    try {
      const profileMetadata = metadataForProfile(selectedProfile);
      const result = await createSession({
        client_type: clientType,
        workspace_id: workspaceId || null,
        handle: handle.trim() || null,
        role: role.trim() || null,
        description: description.trim() || null,
        metadata: selectedProfile ? profileMetadata : {},
        initial_task: initialTask.trim()
          ? { input: initialTask.trim(), metadata: selectedProfile ? profileMetadata : {} }
          : null,
      });
      await loadSessions();
      await selectSession(result.session.session_id);
      handle = '';
      role = '';
      description = '';
      initialTask = '';
      setStatus('Session created.');
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error), true);
    } finally {
      creating = false;
    }
  }
</script>

<section class="panel">
  <h2>Create session</h2>
  <label>Agent profile
    <select bind:value={profileId} on:change={applyProfileDefaults}>
      <option value="">No profile defaults</option>
      {#each $agentProfiles as profile (profile.profile_id)}
        <option value={profile.profile_id}>{profile.name} ({profile.profile_id}@{profile.version})</option>
      {/each}
    </select>
  </label>
  <label>Client type
    <select bind:value={clientType}>
      {#each clientTypeOptions as option (option)}
        <option value={option}>{option}</option>
      {/each}
    </select>
  </label>
  <WorkspaceSelector bind:selectedWorkspaceId={workspaceId} />

  <label>Handle <input bind:value={handle} placeholder="@reviewer" /></label>
  <label>Role <input bind:value={role} placeholder="Optional session role" /></label>
  <label>Description <textarea bind:value={description} placeholder="Optional session description"></textarea></label>
  <label>Initial task <textarea bind:value={initialTask} placeholder="Optional initial task"></textarea></label>
  <button disabled={creating || !workspaceId} on:click={submit}>{creating ? 'Creating...' : 'Create session'}</button>
</section>
