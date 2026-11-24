<script lang="ts">
  import type { Snippet } from 'svelte'
  import { isMobileShell } from '$lib/hooks/shellMode.svelte'
  import { useDeviceRuntime } from '../device/context'
  import { createNewChat } from '../features/chat/newChat.svelte'
  import DesktopNewChat from '../views/desktop/chat/DesktopNewChat.svelte'
  import MobileNewChat from '../views/mobile/chat/MobileNewChat.svelte'
  import WorkspaceOnboarding from '../components/workspaces/WorkspaceOnboarding.svelte'
  let { deviceSelector, routeScope }: { deviceSelector?: Snippet<[boolean]>; routeScope?: string } = $props()
  const chat = createNewChat(useDeviceRuntime(), () => routeScope)
</script>

{#if chat.onboarding && !chat.loading}
  <WorkspaceOnboarding canContinue={!!chat.workspaces.length} onContinue={chat.completeOnboarding} {routeScope} />
{:else if isMobileShell()}<MobileNewChat {chat} />
{:else}<DesktopNewChat {chat} {deviceSelector} />{/if}
