<script lang="ts">
  import { Button } from '$lib/components/ui/button/index.js'
  import PlaceholderPage from './PlaceholderPage.svelte'
</script>

<div class="space-y-4">
  <PlaceholderPage
    title="Page not found"
    eyebrow="404"
    description="This dashboard route is not registered yet."
  />
  <Button href="/dashboard/chat">Back to chat</Button>
</div>
