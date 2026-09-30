<script lang="ts">
  import type { Snippet } from 'svelte';
  import '../app.css';
  import { Toaster } from '$lib/components/ui/sonner/index.js';

  let { children }: { children: Snippet } = $props();
</script>

<Toaster richColors />
{@render children()}
