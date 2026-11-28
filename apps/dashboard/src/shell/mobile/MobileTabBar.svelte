<script lang="ts">
  import { t } from '$lib/i18n'
  import { navigate } from '$lib/navigation'
  import { chromeMetrics } from '$lib/mobile/chromeMetrics'
  import { tapFeedback } from '$lib/mobile/haptic'
  import { tabs } from './tabs'
  let { path, scope, hasOpenTodos, scrollTop }: { path: string; scope?: string; hasOpenTodos: boolean; scrollTop: () => void } = $props()
</script>

<nav data-mobile-toast-anchor use:chromeMetrics={'--mobile-footer-h'} class="mobile-tabs mobile-glass" aria-label={$t('mobile.navigation')}>
  {#each tabs as tab (tab.path)}
    {@const dot = tab.path === '/todos' && hasOpenTodos}
    <button type="button" aria-current={tab.path === path ? 'page' : undefined}
      aria-label={$t(dot ? 'mobile.hasTodos' : tab.label)}
      onclick={() => { if (tab.path === path) scrollTop(); else { tapFeedback(); void navigate(tab.path, undefined, { scope }) } }}>
      <span class="relative"><tab.icon class="size-[22px]" />{#if dot}<span aria-hidden="true" class="absolute -right-2 -top-0.5 size-2 rounded-full bg-primary"></span>{/if}</span>
      {$t(tab.label)}
    </button>
  {/each}
</nav>
