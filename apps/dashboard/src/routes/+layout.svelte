<script lang="ts">
  import type { Snippet } from 'svelte';
  import '../app.css';
  import { QueryClientProvider } from '@tanstack/svelte-query';
  import { Toaster } from '$lib/components/ui/sonner/index.js';
  import { queryClient } from '$lib/queryClient';

  let { children }: { children: Snippet } = $props();
</script>

<QueryClientProvider client={queryClient}>
  <Toaster richColors />
  {@render children()}
</QueryClientProvider>
