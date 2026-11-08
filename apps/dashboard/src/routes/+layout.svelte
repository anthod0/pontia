<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { cleanupDownloads } from '$lib/downloads/save';

  onMount(() => { void cleanupDownloads().catch(console.error); });
  import '../app.css';
  import { Toaster } from '$lib/components/ui/sonner/index.js';

  let { children }: { children: Snippet } = $props();
</script>

<Toaster richColors position="top-center" />
{@render children()}
