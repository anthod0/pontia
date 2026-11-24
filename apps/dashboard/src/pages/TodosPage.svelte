<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import DesktopTodosPage from '../views/desktop/DesktopTodosPage.svelte'
  import MobileTodosPage from '../views/mobile/MobileTodosPage.svelte'
</script>

{#if isMobileShell()}
  <MobileTodosPage />
{:else}
  <DesktopTodosPage />
{/if}
