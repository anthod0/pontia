<script lang="ts">
	import { author, serializeJsonLd, siteOrigin } from '$lib/site';
	import WhereverSection from '$lib/components/WhereverSection.svelte';
	import HeroSection from '$lib/components/HeroSection.svelte';
	import RemoteSection from '$lib/components/RemoteSection.svelte';
	import BotStorySection from '$lib/components/BotStorySection.svelte';
	import OpenSourceSection from '$lib/components/OpenSourceSection.svelte';
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import LocaleLinks from '$lib/components/LocaleLinks.svelte';
	import { m } from '$lib/paraglide/messages';
	import { getLocale, localizeHref } from '$lib/paraglide/runtime';

	const url = `${siteOrigin}${localizeHref('/')}`;
	const image = `${siteOrigin}/og.png`;
	const title = m.home_title();
	const description = m.home_description();

	const structuredData = {
		'@context': 'https://schema.org',
		'@type': 'SoftwareApplication',
		name: 'Pontia',
		url,
		description,
		inLanguage: getLocale(),
		applicationCategory: 'DeveloperApplication',
		operatingSystem: 'Linux, macOS',
		license: 'https://www.apache.org/licenses/LICENSE-2.0',
		codeRepository: 'https://github.com/anthod0/pontia',
		author,
		offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' }
	};
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<meta property="og:site_name" content="Pontia" />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:url" content={url} />
	<meta property="og:image" content={image} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt" content={title} />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:creator" content="@anthod0" />
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content={image} />
	{@html `<script type="application/ld+json">${serializeJsonLd(structuredData)}</script>`}
</svelte:head>

<LocaleLinks />

<div id="homepage">
	<a class="skip-link" href="#main">{m.skip_to_content()}</a>

	<SiteHeader />

	<main id="main">
		<HeroSection />

		<BotStorySection />
		<RemoteSection />
		<OpenSourceSection />
		<WhereverSection />
	</main>

	<SiteFooter />
</div>

<style>
	#homepage :global(:is(button, [role="button"], a.button, a.cta, .skip-link):not(:where(.install-command button))) {
		border-radius: var(--radius-action);
	}
</style>
