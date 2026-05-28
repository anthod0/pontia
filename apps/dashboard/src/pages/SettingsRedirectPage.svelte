<script lang="ts">
  import { onMount } from 'svelte'
  import { navigate } from 'svelte-mini-router'

  onMount(() => {
    navigate('/settings/common')
  })
</script>

<section class="space-y-2">
  <h2 class="text-3xl font-semibold tracking-tight">Settings</h2>
  <p class="text-muted-foreground">Redirecting to common settings…</p>
</section>
