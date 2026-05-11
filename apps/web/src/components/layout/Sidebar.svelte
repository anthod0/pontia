<script lang="ts">
  import AgentProfileList from '../agent-profiles/AgentProfileList.svelte';
  import WorkspaceList from '../workspaces/WorkspaceList.svelte';
  import CreateSessionForm from '../sessions/CreateSessionForm.svelte';
  import SessionList from '../sessions/SessionList.svelte';
</script>

<aside>
  <CreateSessionForm />
  <AgentProfileList />
  <WorkspaceList />
  <SessionList />
</aside>
