<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { currentLocale, updateBrowserLanguage } from '$lib/i18n';
  import { cleanupDownloads } from '$lib/downloads/save';

  onMount(() => { void cleanupDownloads().catch(console.error); });
  import '../app.css';
  import { Toaster } from '$lib/components/ui/sonner/index.js';
  import { applyTheme, resolvedTheme } from '$lib/theme';

  let { children }: { children: Snippet } = $props();

  $effect(() => applyTheme($resolvedTheme));
  $effect(() => { document.documentElement.lang = $currentLocale; });
</script>

<svelte:window onlanguagechange={updateBrowserLanguage} />

<Toaster richColors position="top-center" />
{@render children()}
