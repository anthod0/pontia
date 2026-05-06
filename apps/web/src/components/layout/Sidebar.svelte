<script lang="ts">
  import CreateTaskForm from '../tasks/CreateTaskForm.svelte';
  import TaskList from '../tasks/TaskList.svelte';
  import WorkspaceList from '../workspaces/WorkspaceList.svelte';
  import CreateSessionForm from '../sessions/CreateSessionForm.svelte';
  import SessionList from '../sessions/SessionList.svelte';
</script>

<aside>
  <CreateTaskForm />
  <TaskList />
  <WorkspaceList />
  <SessionList />
  <details class="panel">
    <summary>Compatibility: create session directly</summary>
    <CreateSessionForm />
  </details>
</aside>
