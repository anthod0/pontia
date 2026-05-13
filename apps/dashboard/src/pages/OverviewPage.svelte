<script lang="ts">
  import { Badge } from '$lib/components/ui/badge/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
</script>

<section class="space-y-6">
  <div class="flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
    <div class="space-y-2">
      <Badge variant="secondary">Phase 1 shell</Badge>
      <h2 class="text-3xl font-semibold tracking-tight">Overview</h2>
      <p class="max-w-3xl text-muted-foreground">
        The dashboard frame is ready. Live task, workspace, and profile summaries will be added in Phase 2.
      </p>
    </div>
  </div>

  <div class="grid gap-4 md:grid-cols-3">
    <Card.Root>
      <Card.Header>
        <Card.Title>DAG Tasks</Card.Title>
        <Card.Description>Primary workflow entrypoint</Card.Description>
      </Card.Header>
      <Card.Content class="text-3xl font-semibold">—</Card.Content>
    </Card.Root>
    <Card.Root>
      <Card.Header>
        <Card.Title>Workspaces</Card.Title>
        <Card.Description>Execution contexts</Card.Description>
      </Card.Header>
      <Card.Content class="text-3xl font-semibold">—</Card.Content>
    </Card.Root>
    <Card.Root>
      <Card.Header>
        <Card.Title>Agent Profiles</Card.Title>
        <Card.Description>Runtime capabilities</Card.Description>
      </Card.Header>
      <Card.Content class="text-3xl font-semibold">—</Card.Content>
    </Card.Root>
  </div>
</section>
