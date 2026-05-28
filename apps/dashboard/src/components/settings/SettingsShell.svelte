<script lang="ts">
  import type { Snippet } from 'svelte'
  import SettingsSectionNav from './SettingsSectionNav.svelte'

  let { children }: { children: Snippet } = $props()
</script>

<div class="flex flex-col gap-6 md:flex-row md:items-start">
  <SettingsSectionNav />
  <div class="min-w-0 flex-1">
    {@render children()}
  </div>
</div>
