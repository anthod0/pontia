<script lang="ts">
	import SiteFooter from '$lib/components/SiteFooter.svelte';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import LocaleLinks from '$lib/components/LocaleLinks.svelte';
	import { formatDate } from '$lib/blog';
	import { m } from '$lib/paraglide/messages';
	import { localizeHref } from '$lib/paraglide/runtime';

	let { data } = $props();

	const url = `https://pontia.dev${localizeHref('/blog')}`;
	const title = m.blog_title();
	const description = m.blog_description();
</script>

<svelte:head>
	<title>{title}</title>
	<meta name="description" content={description} />
	<meta property="og:site_name" content="Pontia" />
	<meta property="og:title" content={title} />
	<meta property="og:description" content={description} />
	<meta property="og:type" content="website" />
	<meta property="og:url" content={url} />
	<meta property="og:image" content="https://pontia.dev/og.png" />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:title" content={title} />
	<meta name="twitter:description" content={description} />
	<meta name="twitter:image" content="https://pontia.dev/og.png" />
</svelte:head>

<LocaleLinks />

<a class="skip-link" href="#blog">{m.skip_to_content()}</a>

<SiteHeader />

<main id="blog" class="blog page-width">
	<h1>{m.blog_heading()}</h1>
	<p class="intro">{description}</p>
	<ul>
		{#each data.posts as post (post.slug)}
			<li>
				<a href={localizeHref(`/blog/${post.slug}`)}>
					<time datetime={post.published}>{formatDate(post.published)}</time>
					<h2>{post.title}</h2>
					<p>{post.description}</p>
				</a>
			</li>
		{/each}
	</ul>
</main>

<SiteFooter />

<style>
	.blog { max-width: 720px; padding-block: 64px 96px; }
	h1 { font-size: clamp(36px, 5vw, 52px); font-weight: 500; line-height: 1.1; letter-spacing: -0.045em; }
	.intro { margin-top: 16px; color: var(--muted-foreground); font-size: 15px; line-height: 1.75; }
	ul { margin: 40px 0 0; padding: 0; list-style: none; border-top: 1px solid var(--border); }
	li { border-bottom: 1px solid var(--border); }
	li a { display: grid; gap: 8px; padding-block: 28px; }
	time { color: var(--muted-foreground); font: 12px var(--font-mono); }
	h2 { font-size: 21px; line-height: 1.35; letter-spacing: -0.02em; }
	li a:hover h2 { color: var(--primary); }
	li p { color: var(--muted-foreground); font-size: 14px; line-height: 1.7; }
	@media (max-width: 760px) {
		.blog { padding-block: 32px 64px; }
		h2 { font-size: 19px; }
	}
</style>
