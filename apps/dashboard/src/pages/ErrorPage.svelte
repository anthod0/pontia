<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { dashboardPath } from '$lib/dashboardRoutes'

  let { status, message }: { status: number; message?: string } = $props()

  const notFound = $derived(status === 404)
</script>

<main class="flex min-h-svh p-6">
  <Empty.Root>
    <Empty.Header>
      <p class="text-sm font-medium text-muted-foreground">{status}</p>
      <Empty.Title class="text-2xl">{notFound ? 'Page not found' : 'Something went wrong'}</Empty.Title>
      <Empty.Description>
        {notFound ? "This page doesn't exist or has been moved." : message || 'An unexpected error occurred.'}
      </Empty.Description>
    </Empty.Header>
    <Empty.Content>
      <Button href={dashboardPath('/')}>Back to chat</Button>
    </Empty.Content>
  </Empty.Root>
</main>
