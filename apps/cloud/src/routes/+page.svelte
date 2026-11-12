<script lang="ts">
	import { siteOrigin } from '$lib/site';
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import HeroSection from '$lib/components/HeroSection.svelte';
	import FeatureSection from '$lib/components/FeatureSection.svelte';
	import HomeFaq from '$lib/components/HomeFaq.svelte';
	import OpenSourceSection from '$lib/components/OpenSourceSection.svelte';
	import IntegrationProgress from '$lib/components/IntegrationProgress.svelte';
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import LocaleLinks from '$lib/components/LocaleLinks.svelte';
	import { reveal } from '$lib/reveal';
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
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content={image} />
	{@html `<script type="application/ld+json">${JSON.stringify(structuredData)}</script>`}
</svelte:head>

<LocaleLinks />

<a class="skip-link" href="#main">{m.skip_to_content()}</a>

<SiteHeader />

<main id="main">
	<HeroSection />

	<FeatureSection />
	<OpenSourceSection />
	<IntegrationProgress />
	<HomeFaq />

	<section class="closing page-width" aria-labelledby="closing-title" {@attach reveal()}>
		<h2 id="closing-title" class="reveal">{m.home_closing_title()}</h2>
		<p class="reveal" style="--i: 1">{m.home_closing_text()}</p>
		<div class="closing-actions reveal" style="--i: 2">
			<a class="button" href="https://app.pontia.dev">{m.get_started()}<ArrowRightIcon size={18} /></a>
			<a class="button button-secondary" href={localizeHref('/docs/getting-started')}>{m.setup_guide()}</a>
		</div>
	</section>
</main>

<SiteFooter />

<style>
	/* Closing call to action above the footer, on a soft brand glow. */
	.closing { display: flex; flex-direction: column; align-items: center; padding-block: 120px 128px; text-align: center; background: radial-gradient(ellipse 50% 60% at 50% 55%, rgb(69 133 136 / 10%), transparent 70%); }
	.closing h2 { font-size: clamp(32px, 4.4vw, 52px); letter-spacing: -0.045em; }
	.closing p { margin-top: 16px; font-size: 16px; color: var(--muted-foreground); }
	.closing-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin-top: 32px; }
	.closing-actions .button-secondary { background: var(--background); }
	@media (max-width: 600px) {
		.closing { padding-block: 72px 80px; }
		.closing p { font-size: 14px; }
	}
</style>
