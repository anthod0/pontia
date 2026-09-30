<script lang="ts">
  import * as Card from '$lib/components/ui/card/index.js';
</script>

<section class="space-y-6">
  <div class="space-y-2">
    <h2 class="text-3xl font-semibold tracking-tight">Common settings</h2>
    <p class="max-w-3xl text-muted-foreground">Connection settings are managed by Pontia for remote Dashboards.</p>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>Remote connection</Card.Title>
      <Card.Description>This Dashboard does not use a local External API credential.</Card.Description>
    </Card.Header>
  </Card.Root>
</section>
