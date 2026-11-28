<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

<ul class="mt-4 divide-y divide-border border-y border-border">
  {@render children()}
</ul>

<style>
  ul :global(> li) {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 20px;
    padding-block: 16px;
  }
  @media (max-width: 600px) {
    ul :global(> li) {
      grid-template-columns: 1fr;
    }
  }
</style>
