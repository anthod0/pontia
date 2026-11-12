<script lang="ts">
	import { siteOrigin } from '$lib/site';
	import { onMount, type Snippet } from 'svelte';
	import { page } from '$app/state';
	import ArrowLeftIcon from 'phosphor-svelte/lib/ArrowLeftIcon';
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import LocaleLinks from '$lib/components/LocaleLinks.svelte';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import { addCodeCopyButtons } from '$lib/code-copy';
	import { formatDate, type BlogPostMetadata } from '$lib/blog';
	import '$lib/styles/article.css';
	import { m } from '$lib/paraglide/messages';
	import { getLocale, localizeHref } from '$lib/paraglide/runtime';

	let { title, description, published, updated, children }: BlogPostMetadata & { children: Snippet } =
		$props();

	const url = $derived(`${siteOrigin}${page.url.pathname}`);
	const image = `${siteOrigin}/og.png`;
	const pontia = { '@type': 'Organization', name: 'Pontia', url: `${siteOrigin}/` };
	const structuredData = $derived({
		'@context': 'https://schema.org',
		'@type': 'Article',
		headline: title,
		inLanguage: getLocale(),
		description,
		datePublished: published,
		dateModified: updated ?? published,
		image,
		url,
		mainEntityOfPage: url,
		author: pontia,
		publisher: { ...pontia, logo: { '@type': 'ImageObject', url: `${siteOrigin}/logo.svg` } }
	});

	let article: HTMLElement;
	onMount(() => addCodeCopyButtons(article));
</script>

<svelte:head>
	<title>{title} — Pontia</title>
	<meta name="description" content={description} />
	<meta property="og:site_name" content="Pontia" />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="article" />
	<meta property="og:url" content={url} />
	<meta property="og:image" content={image} />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="article:published_time" content={published} />
	<meta property="article:modified_time" content={updated ?? published} />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content={image} />
	{@html `<script type="application/ld+json">${JSON.stringify(structuredData).replaceAll('<', '\\u003c')}</script>`}
</svelte:head>

<LocaleLinks />

<a class="skip-link" href="#post">{m.skip_to_content()}</a>

<SiteHeader />

<main id="post" class="post page-width">
	<a class="back" href={localizeHref('/blog')}><ArrowLeftIcon size={14} />{m.nav_blog()}</a>
	<article class="article-content" bind:this={article}>
		<header>
			<h1>{title}</h1>
			<p class="dates">
				<time datetime={published}>{formatDate(published)}</time>
				{#if updated}
					<span>· <time datetime={updated}>{m.blog_updated({ date: formatDate(updated) })}</time></span>
				{/if}
			</p>
		</header>
		{@render children()}
	</article>
	<aside class="cta">
		<p>{m.blog_cta()}</p>
		<a class="button" href={localizeHref('/docs/getting-started')}>{m.get_started()}<ArrowRightIcon size={18} /></a>
	</aside>
</main>

<SiteFooter />

<style>
	.post { max-width: 720px; padding-block: 56px 96px; }
	.back { display: inline-flex; align-items: center; gap: 6px; margin-bottom: 28px; color: var(--muted-foreground); font-size: 13px; }
	.back:hover { color: var(--primary); }
	header { margin-bottom: 32px; }
	.article-content .dates { margin: 0; color: var(--muted-foreground); font: 12px var(--font-mono); }
	.cta { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 20px; margin-top: 56px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); padding: 24px; }
	.cta p { max-width: 400px; color: var(--foreground); font-size: 15px; line-height: 1.6; }
	@media (max-width: 760px) {
		.post { padding-block: 32px 64px; }
	}
</style>
