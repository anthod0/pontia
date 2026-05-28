<script lang="ts">
  import type { Snippet } from 'svelte'
  import SettingsSectionNav from './SettingsSectionNav.svelte'

  let { children }: { children: Snippet } = $props()
</script>

<div class="flex flex-col gap-6 md:flex-row md:items-start">
  <SettingsSectionNav />
  <div class="flex min-w-0 flex-1 justify-center px-4 py-4 md:px-6 md:py-6">
    <div class="mx-auto w-full max-w-5xl">
      {@render children()}
    </div>
  </div>
</div>
