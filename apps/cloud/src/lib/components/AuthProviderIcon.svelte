<script lang="ts">
  // Google G extracted from the icon-only SVG in https://developers.google.com/static/identity/images/signin-assets.zip.
  // GitHub marks: https://brand.github.com/GitHub_Logos.zip.
  import google from '$lib/assets/auth/google.svg?raw';
  import githubLight from '$lib/assets/auth/github-light.svg';
  import githubDark from '$lib/assets/auth/github-dark.svg';

  let { provider }: { provider: 'google' | 'github' } = $props();
</script>

{#if provider === 'google'}
  <span class="inline-flex size-5 shrink-0 [&>svg]:size-full" aria-hidden="true">{@html google}</span>
{:else}
  <img class="size-5 shrink-0 object-contain dark:hidden" src={githubLight} alt="" width="20" height="20" />
  <img class="hidden size-5 shrink-0 object-contain dark:block" src={githubDark} alt="" width="20" height="20" />
{/if}
