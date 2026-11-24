<script lang="ts">
  import { onMount, type Snippet } from 'svelte'
  import DesktopShell from './desktop/DesktopShell.svelte'

  import {provideSessionPanel} from '../components/chat/sessionPanel/sessionPanelContext.svelte'
  import MobileShell from './mobile/MobileShell.svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { installVisualViewportCssVars } from '$lib/visualViewport'
  provideSessionPanel()
  onMount(() => installVisualViewportCssVars())
  let { children }: { children: Snippet } = $props()
</script>

{#if isMobileShell()}
  <MobileShell>{@render children()}</MobileShell>
{:else}
  <DesktopShell>{@render children()}</DesktopShell>
{/if}
