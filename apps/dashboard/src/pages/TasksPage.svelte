<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import PlaceholderPage from './PlaceholderPage.svelte'
</script>

<div class="space-y-4">
  <PlaceholderPage
    title="DAG Tasks"
    description="Create, list, and inspect DAG-managed tasks. This page is intentionally DAG-first; sessions appear only inside task diagnostics."
    details={[
      'Phase 3 will call /external/v1/dag-tasks for creation.',
      'Task detail tabs are already routable and refreshable.',
    ]}
  />
  <Button href="/dashboard/tasks/example/dag" variant="outline">Open example DAG route</Button>
</div>
