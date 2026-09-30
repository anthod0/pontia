<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import { dashboardPath } from '$lib/dashboardRoutes'
  import PlaceholderPage from './PlaceholderPage.svelte'
</script>

<div class="space-y-4">
  <PlaceholderPage
    title="Page not found"
    eyebrow="404"
    description="This dashboard route is not registered yet."
  />
  <Button href={dashboardPath('/')}>Back to chat</Button>
</div>
