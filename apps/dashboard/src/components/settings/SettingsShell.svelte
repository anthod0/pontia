<script lang="ts">
  import type { Snippet } from 'svelte'
  import SettingsSectionNav from './SettingsSectionNav.svelte'

  let { children }: { children: Snippet } = $props()
</script>

<div class="mx-auto flex w-full max-w-[1286px] flex-col gap-9 px-4 py-9 md:flex-row md:items-start md:gap-[72px] md:px-6 md:py-16">
  <SettingsSectionNav />
  <div class="flex min-w-0 flex-1 justify-center">
    <div class="mx-auto w-full max-w-5xl">
      {@render children()}
    </div>
  </div>
</div>
