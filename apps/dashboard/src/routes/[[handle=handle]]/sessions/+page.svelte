<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import MobileSessionsPage from '../../../views/mobile/MobileSessionsPage.svelte'
  import SessionsPage from '../../../pages/SessionsPage.svelte'
</script>

{#if isMobileShell()}<MobileSessionsPage />{:else}<SessionsPage />{/if}
