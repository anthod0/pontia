<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { cleanupDownloads } from '$lib/downloads/save';

  onMount(() => { void cleanupDownloads().catch(console.error); });
  import '../app.css';
  import { QueryClientProvider } from '@tanstack/svelte-query';
  import { Toaster } from '$lib/components/ui/sonner/index.js';
  import { queryClient } from '../queries/queryClient';

  let { children }: { children: Snippet } = $props();
</script>

<QueryClientProvider client={queryClient}>
  <Toaster richColors position="top-center" />
  {@render children()}
</QueryClientProvider>
