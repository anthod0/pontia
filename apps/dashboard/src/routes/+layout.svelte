<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { currentLocale, updateBrowserLanguage } from '$lib/i18n';
  import { cleanupDownloads } from '$lib/downloads/save';
  import { openDashboardTab } from '$lib/lastPage';

  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  openDashboardTab();

  onMount(() => { void cleanupDownloads().catch(console.error); });
  import '../app.css';
  import { Toaster } from '$lib/components/ui/sonner/index.js';
  import { applyTheme, resolvedTheme } from '$lib/theme';

  let { children }: { children: Snippet } = $props();

  $effect(() => applyTheme($resolvedTheme));
  $effect(() => { document.documentElement.lang = $currentLocale; });
</script>

<svelte:window onlanguagechange={updateBrowserLanguage} />

<Toaster
  position={isMobileShell() ? 'bottom-center' : 'top-center'}
  visibleToasts={isMobileShell() ? 1 : 3}
  gap={8}
  pauseWhenPageIsHidden
  swipeDirections={isMobileShell() ? ['bottom', 'left', 'right'] : ['top']}
  offset={{ top: '56px' }}
  mobileOffset={{ bottom: 'var(--mobile-toast-bottom, calc(var(--visual-viewport-bottom, 0px) + max(12px, env(safe-area-inset-bottom))))' }} />
{@render children()}
