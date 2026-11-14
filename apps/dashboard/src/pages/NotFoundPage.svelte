<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { dashboardPath } from '$lib/dashboardRoutes'
</script>

<main class="flex min-h-svh p-6">
  <Empty.Root>
    <Empty.Header>
      <p class="text-sm font-medium text-muted-foreground">404</p>
      <Empty.Title class="text-2xl">Page not found</Empty.Title>
      <Empty.Description>This page doesn't exist or has been moved.</Empty.Description>
    </Empty.Header>
    <Empty.Content>
      <Button href={dashboardPath('/')}>Back to chat</Button>
    </Empty.Content>
  </Empty.Root>
</main>
