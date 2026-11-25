<script lang="ts">
  import { onMount } from 'svelte'
  import { dispatchShortcut } from '$lib/shortcuts/dispatch'
  import { shortcuts } from '$lib/shortcuts/store.svelte'
  onMount(shortcuts.sync)
</script>

<svelte:window onkeydown={dispatchShortcut} />
