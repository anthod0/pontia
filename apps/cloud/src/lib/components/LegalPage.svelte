<script lang="ts">
  import "$lib/styles/article.css";
  import SiteFooter from "$lib/components/SiteFooter.svelte";
  import SiteHeader from "$lib/components/SiteHeader.svelte";
  import type { Snippet } from "svelte";
  import { m } from "$lib/paraglide/messages";

  let {
    title,
    description,
    children,
  }: { title: string; description: string; children: Snippet } = $props();
</script>

<svelte:head>
  <title>{title} — Pontia</title>
  <meta name="description" content={description} />
</svelte:head>

<a class="skip-link" href="#legal-content">{m.skip_to_content()}</a>
<SiteHeader />
<main id="legal-content" class="legal-content article-content page-width">
  {@render children()}
</main>
<SiteFooter />

<style>
  .legal-content {
    max-width: 780px;
    padding-block: 72px 96px;
  }
  .legal-content :global(.effective-date) {
    margin-bottom: 48px;
    color: var(--muted-foreground);
    font: 12px/1.6 var(--font-mono);
  }
  @media (max-width: 600px) {
    .legal-content {
      padding-block: 48px 64px;
    }
    .legal-content :global(.effective-date) {
      margin-bottom: 36px;
    }
  }
</style>
