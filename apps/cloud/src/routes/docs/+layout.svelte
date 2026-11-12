<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import CaretDownIcon from 'phosphor-svelte/lib/CaretDownIcon';
	import LocaleLinks from '$lib/components/LocaleLinks.svelte';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import * as Popover from '$lib/components/ui/popover';
	import { tick, type Snippet } from 'svelte';
	import { addCodeCopyButtons } from '$lib/code-copy';
	import '$lib/styles/article.css';
	import { m } from '$lib/paraglide/messages';
	import { deLocalizeHref, localizeHref } from '$lib/paraglide/runtime';

	let { children }: { children: Snippet } = $props();

	const sections = [
		{
			title: m.docs_group_overview,
			links: [
				{ href: '/docs/getting-started', title: m.docs_getting_started },
				{ href: '/docs/initialization', title: m.docs_initialization },
				{ href: '/docs/how-it-works', title: m.how_pontia_works }
			]
		},
		{
			title: m.docs_group_remote_access,
			links: [
				{ href: '/docs/remote-access', title: m.docs_remote_access },
				{ href: '/docs/device-management', title: m.docs_device_management },
				{ href: '/docs/self-hosted-edge', title: m.docs_self_hosted_edge },
				{ href: '/docs/security', title: m.docs_security }
			]
		},
		{
			title: m.docs_group_using,
			links: [
				{ href: '/docs/dashboard', title: m.docs_dashboard },
				{ href: '/docs/agent-clients', title: m.agent_clients },
				{ href: '/docs/codex', title: () => 'Codex' },
				{ href: '/docs/workspaces', title: m.docs_workspaces },
				{ href: '/docs/workflows', title: m.docs_workflows }
			]
		},
		{
			title: m.docs_group_reference,
			links: [
				{ href: '/docs/configuration', title: m.docs_configuration },
				{ href: '/docs/cli', title: () => 'CLI' },
				{ href: '/docs/updating', title: m.docs_updating },
				{ href: '/docs/platforms/linux', title: () => 'Linux' },
				{ href: '/docs/platforms/macos', title: () => 'macOS' },
				{ href: '/docs/troubleshooting', title: m.docs_troubleshooting },
				{ href: '/docs/faq', title: m.docs_faq }
			]
		}
	];
	const pathname = $derived(deLocalizeHref(page.url.pathname));
	const currentTitle = $derived(
		(sections.flatMap((section) => section.links).find((link) => link.href === pathname)?.title ?? m.docs_contents)()
	);
	// On narrow screens the section list lives in a popover behind a sticky bar.
	let menuOpen = $state(false);

	afterNavigate(async () => {
		menuOpen = false;
		await tick();
		addCodeCopyButtons(document.querySelector('.docs-content')!);
	});
</script>

{#snippet navGroups()}
	{#each sections as section}
		<div class="nav-group">
			<span>{section.title()}</span>
			{#each section.links as link}
				<a href={localizeHref(link.href)} aria-current={pathname === link.href ? 'page' : undefined}>{link.title()}</a>
			{/each}
		</div>
	{/each}
{/snippet}

<LocaleLinks />

<a class="skip-link" href="#docs-content">{m.skip_to_content()}</a>

<SiteHeader />

<div class="docs-shell page-width">
	<aside>
		<a class="docs-title" href={localizeHref('/docs')}>{m.docs_heading()}</a>
		<nav aria-label={m.docs_sections()}>{@render navGroups()}</nav>
	</aside>
	<div class="docs-menu">
		<Popover.Root bind:open={menuOpen}>
			<Popover.Trigger class="docs-menu-toggle">
				<span>{currentTitle}</span>
				<CaretDownIcon size={16} />
			</Popover.Trigger>
			<Popover.Content class="docs-menu-content" align="start" sideOffset={6}>
				<nav aria-label={m.docs_sections()}>{@render navGroups()}</nav>
			</Popover.Content>
		</Popover.Root>
	</div>
	<main id="docs-content" class="docs-content article-content">{@render children()}</main>
</div>

<style>
	.docs-shell { display: grid; grid-template-columns: 210px minmax(0, 720px); gap: 64px; padding-block: 64px 96px; }
	aside { position: sticky; top: 32px; max-height: calc(100vh - 64px); align-self: start; overflow-y: auto; }
	.docs-title { display: block; margin-bottom: 20px; color: var(--heading); font-size: 14px; font-weight: 600; }
	aside nav { display: grid; gap: 20px; border-left: 1px solid var(--border); padding-left: 16px; color: var(--muted-foreground); font-size: 13px; }
	.nav-group { display: grid; gap: 9px; }
	.nav-group span { color: var(--heading); font-size: 10px; font-weight: 600; letter-spacing: 0.09em; text-transform: uppercase; }
	nav a:hover, nav a[aria-current='page'] { color: var(--primary); }
	.docs-menu { display: none; }
	.docs-content { min-width: 0; }

	@media (max-width: 760px) {
		.docs-shell { grid-template-columns: 1fr; gap: 28px; padding-block: 24px 64px; }
		aside { display: none; }
		/* Sticky bar that stays reachable while reading; the list opens in a popover. */
		.docs-menu { position: sticky; top: 0; z-index: 20; display: block; margin-inline: -32px; padding: 10px 32px; background: var(--background); border-bottom: 1px solid var(--border); }
		.docs-menu :global(.docs-menu-toggle) { display: flex; align-items: center; justify-content: space-between; width: 100%; padding: 10px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); color: var(--heading); font-size: 14px; font-weight: 500; }
		.docs-menu :global(.docs-menu-toggle svg) { transition: transform 0.15s; }
		.docs-menu :global(.docs-menu-toggle[data-state='open'] svg) { transform: rotate(180deg); }
		.docs-content :global(:is(h2, h3)) { scroll-margin-top: 72px; }
	}

	/* Popover content is portalled to <body>, so these rules are global. */
	:global(.docs-menu-content) { width: var(--bits-floating-anchor-width); max-height: min(70vh, calc(var(--bits-floating-available-height) - 16px)); overflow-y: auto; padding: 18px 16px; border-radius: 6px; }
	:global(.docs-menu-content nav) { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 20px 16px; color: var(--muted-foreground); font-size: 14px; }
	:global(.docs-menu-content .nav-group) { display: grid; align-content: start; gap: 10px; }
	:global(.docs-menu-content .nav-group span) { color: var(--heading); font-size: 10px; font-weight: 600; letter-spacing: 0.09em; text-transform: uppercase; }
	:global(.docs-menu-content a:hover), :global(.docs-menu-content a[aria-current='page']) { color: var(--primary); }

	@media (max-width: 600px) {
		.docs-menu { margin-inline: -20px; padding-inline: 20px; }
	}
</style>
