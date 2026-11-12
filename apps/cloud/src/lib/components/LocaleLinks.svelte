<script lang="ts">
	import { page } from '$app/state';
	import { hreflangs, ogLocales } from '$lib/locales';
	import { deLocalizeHref, getLocale, locales, localizeHref } from '$lib/paraglide/runtime';

	const origin = 'https://pontia.dev';
	const path = $derived(deLocalizeHref(page.url.pathname));
	const locale = $derived(getLocale());
</script>

<!-- Canonical, language alternates, and Open Graph locale for a page published in every locale. -->
<svelte:head>
	<link rel="canonical" href={`${origin}${page.url.pathname}`} />
	{#each locales as alternate (alternate)}
		<link rel="alternate" hreflang={hreflangs[alternate]} href={`${origin}${localizeHref(path, { locale: alternate })}`} />
	{/each}
	<link rel="alternate" hreflang="x-default" href={`${origin}${path}`} />
	<meta property="og:locale" content={ogLocales[locale]} />
	{#each locales.filter((alternate) => alternate !== locale) as alternate (alternate)}
		<meta property="og:locale:alternate" content={ogLocales[alternate]} />
	{/each}
</svelte:head>
