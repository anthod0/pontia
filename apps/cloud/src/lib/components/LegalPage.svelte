<script lang="ts">
  import SiteFooter from "$lib/components/SiteFooter.svelte";
  import SiteHeader from "$lib/components/SiteHeader.svelte";
  import type { Snippet } from "svelte";

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

<a class="skip-link" href="#legal-content">Skip to content</a>
<SiteHeader />
<main id="legal-content" class="legal-content page-width">
  {@render children()}
</main>
<SiteFooter />

<style>
  .legal-content {
    max-width: 780px;
    padding-block: 72px 96px;
  }
  .legal-content :global(h1) {
    margin-bottom: 10px;
    font-size: clamp(36px, 6vw, 52px);
    font-weight: 500;
    letter-spacing: -0.045em;
  }
  .legal-content :global(.effective-date) {
    margin-bottom: 48px;
    color: var(--muted-foreground);
    font: 12px/1.6 var(--font-mono);
  }
  .legal-content :global(h2) {
    margin: 42px 0 14px;
    font-size: 22px;
    letter-spacing: -0.025em;
  }
  .legal-content :global(h3) {
    margin: 24px 0 10px;
    font-size: 16px;
  }
  .legal-content :global(p),
  .legal-content :global(li) {
    font-size: 15px;
    line-height: 1.8;
  }
  .legal-content :global(p + p) {
    margin-top: 14px;
  }
  .legal-content :global(ul) {
    margin: 14px 0;
    padding-left: 24px;
  }
  .legal-content :global(li + li) {
    margin-top: 8px;
  }
  .legal-content :global(a) {
    color: var(--primary);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .legal-content :global(strong) {
    color: var(--heading);
  }

  @media (max-width: 600px) {
    .legal-content {
      padding-block: 48px 64px;
    }
    .legal-content :global(.effective-date) {
      margin-bottom: 36px;
    }
    .legal-content :global(h2) {
      margin-top: 34px;
    }
  }
</style>
