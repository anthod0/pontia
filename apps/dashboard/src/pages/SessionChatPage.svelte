<script lang="ts">
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { createSessionChat } from '../features/sessions/sessionChat.svelte'
  import DesktopSessionChat from '../views/desktop/sessions/DesktopSessionChat.svelte'
  import MobileSessionChat from '../views/mobile/sessions/MobileSessionChat.svelte'
  let { routeSessionId = null }: { routeSessionId?: string | null } = $props()
  const chat = createSessionChat(() => routeSessionId)
</script>

{#if isMobileShell()}<MobileSessionChat {chat} />{:else}<DesktopSessionChat {chat} />{/if}
