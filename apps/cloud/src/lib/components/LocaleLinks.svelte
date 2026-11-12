<script lang="ts">
	import { page } from '$app/state';
	import { localeInfo } from '$lib/locales';
	import { deLocalizeHref, getLocale, locales, localizeHref } from '$lib/paraglide/runtime';
	import { siteOrigin } from '$lib/site';

	const path = $derived(deLocalizeHref(page.url.pathname));
	const locale = $derived(getLocale());
</script>

<!-- Canonical, language alternates, and Open Graph locale for a page published in every locale. -->
<svelte:head>
	<link rel="canonical" href={`${siteOrigin}${page.url.pathname}`} />
	{#each locales as alternate (alternate)}
		<link rel="alternate" hreflang={alternate} href={`${siteOrigin}${localizeHref(path, { locale: alternate })}`} />
	{/each}
	<link rel="alternate" hreflang="x-default" href={`${siteOrigin}${path}`} />
	<meta property="og:locale" content={localeInfo[locale].og} />
	{#each locales.filter((alternate) => alternate !== locale) as alternate (alternate)}
		<meta property="og:locale:alternate" content={localeInfo[alternate].og} />
	{/each}
</svelte:head>
