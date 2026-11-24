<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileSessionsPage from '../views/mobile/sessions/MobileSessionsPage.svelte'
  import DesktopSessions from '../views/desktop/sessions/DesktopSessions.svelte'
</script>

{#if isMobileShell()}<MobileSessionsPage />{:else}<DesktopSessions />{/if}
