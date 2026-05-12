<script lang="ts">
  import AgentProfileList from '../agent-profiles/AgentProfileList.svelte';
  import WorkspaceList from '../workspaces/WorkspaceList.svelte';
  import CreateSessionForm from '../sessions/CreateSessionForm.svelte';
  import SessionList from '../sessions/SessionList.svelte';
  import CreateTaskForm from '../tasks/CreateTaskForm.svelte';
  import TaskList from '../tasks/TaskList.svelte';

  export let view = 'sessions' as 'sessions' | 'tasks';
</script>

<aside>
  {#if view === 'tasks'}
    <CreateTaskForm />
    <TaskList />
  {:else}
    <CreateSessionForm />
    <AgentProfileList />
    <WorkspaceList />
    <SessionList />
  {/if}
</aside>
