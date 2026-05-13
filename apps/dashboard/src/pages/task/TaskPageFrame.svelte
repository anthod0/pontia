<script lang="ts">
  import { getPathParams, navigate } from 'svelte-mini-router'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Card from '$lib/components/ui/card/index.js'

  let {
    title,
    description,
  }: {
    title: string
    description: string
  } = $props()

  const { taskId = 'unknown' } = getPathParams()
  const tabs = [
    ['Overview', `/tasks/${taskId}/overview`],
    ['DAG', `/tasks/${taskId}/dag`],
    ['Work Items', `/tasks/${taskId}/work-items`],
    ['Sessions', `/tasks/${taskId}/sessions`],
    ['Artifacts', `/tasks/${taskId}/artifacts`],
    ['Activity', `/tasks/${taskId}/activity`],
  ] as const
</script>

<section class="space-y-6">
  <div class="space-y-2">
    <p class="text-sm font-medium text-muted-foreground">Task {taskId}</p>
    <h2 class="text-3xl font-semibold tracking-tight">{title}</h2>
    <p class="max-w-3xl text-muted-foreground">{description}</p>
  </div>

  <div class="flex flex-wrap gap-2">
    {#each tabs as [label, path]}
      <Button variant="outline" size="sm" onclick={() => navigate(path)}>{label}</Button>
    {/each}
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>{title}</Card.Title>
      <Card.Description>Placeholder for shareable task detail route.</Card.Description>
    </Card.Header>
    <Card.Content class="text-sm text-muted-foreground">
      Refresh this URL under <code>/dashboard/tasks/{taskId}/...</code> to verify the backend SPA fallback serves <code>index.html</code>.
    </Card.Content>
  </Card.Root>
</section>
