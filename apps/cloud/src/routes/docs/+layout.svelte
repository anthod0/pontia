<script lang="ts">
	import { afterNavigate } from '$app/navigation';
	import SiteHeader from '$lib/components/SiteHeader.svelte';
	import { tick, type Snippet } from 'svelte';

	let { children }: { children: Snippet } = $props();

	afterNavigate(async () => {
		await tick();
		for (const block of document.querySelectorAll<HTMLElement>('.docs-content pre')) {
			if (block.dataset.copyable === 'true') continue;
			const code = block.querySelector('code');
			if (!code) continue;

			block.dataset.copyable = 'true';
			const button = document.createElement('button');
			button.type = 'button';
			button.className = 'code-copy-button';
			button.textContent = 'Copy';
			button.setAttribute('aria-label', 'Copy code');
			button.addEventListener('click', async () => {
				try {
					await navigator.clipboard.writeText(code.textContent ?? '');
					button.textContent = 'Copied';
					button.setAttribute('aria-label', 'Copy code again');
					setTimeout(() => {
						button.textContent = 'Copy';
						button.setAttribute('aria-label', 'Copy code');
					}, 2000);
				} catch {
					button.textContent = 'Failed';
					setTimeout(() => {
						button.textContent = 'Copy';
					}, 2000);
				}
			});
			block.appendChild(button);
		}
	});
</script>

<a class="skip-link" href="#docs-content">Skip to content</a>

<SiteHeader />

<div class="docs-shell page-width">
	<aside>
		<a class="docs-title" href="/docs">Documentation</a>
		<nav aria-label="Documentation sections">
			<div class="nav-group">
				<span>Overview</span>
				<a href="/docs/getting-started">Getting started</a>
				<a href="/docs/how-it-works">How Pontia works</a>
			</div>
			<div class="nav-group">
				<span>Remote access</span>
				<a href="/docs/remote-access">Remote access</a>
				<a href="/docs/device-management">Device management</a>
				<a href="/docs/self-hosted-edge">Self-host an Edge</a>
				<a href="/docs/security">Security</a>
			</div>
			<div class="nav-group">
				<span>Using Pontia</span>
				<a href="/docs/dashboard">Dashboard</a>
				<a href="/docs/agent-clients">Agent Clients</a>
				<a href="/docs/workspaces">Workspaces</a>
				<a href="/docs/workflows">Workflows</a>
			</div>
			<div class="nav-group">
				<span>Reference</span>
				<a href="/docs/configuration">Configuration</a>
				<a href="/docs/cli">CLI</a>
				<a href="/docs/updating">Updating</a>
				<a href="/docs/platforms/linux">Linux</a>
				<a href="/docs/platforms/macos">macOS</a>
				<a href="/docs/troubleshooting">Troubleshooting</a>
				<a href="/docs/faq">FAQ</a>
			</div>
		</nav>
	</aside>
	<main id="docs-content" class="docs-content">{@render children()}</main>
</div>

<style>
	.docs-shell { display: grid; grid-template-columns: 210px minmax(0, 720px); gap: 64px; padding-block: 64px 96px; }
	aside { position: sticky; top: 32px; max-height: calc(100vh - 64px); align-self: start; overflow-y: auto; }
	.docs-title { display: block; margin-bottom: 20px; color: var(--heading); font-size: 14px; font-weight: 600; }
	aside nav { display: grid; gap: 20px; border-left: 1px solid var(--border); padding-left: 16px; color: var(--muted-foreground); font-size: 13px; }
	.nav-group { display: grid; gap: 9px; }
	.nav-group span { color: var(--heading); font-size: 10px; font-weight: 600; letter-spacing: 0.09em; text-transform: uppercase; }
	aside nav a:hover { color: var(--primary); }
	.docs-content { min-width: 0; }
	.docs-content :global(h1) { margin-bottom: 18px; color: var(--heading); font-size: clamp(36px, 5vw, 52px); font-weight: 500; line-height: 1.1; letter-spacing: -0.045em; }
	.docs-content :global(h2) { margin: 48px 0 16px; padding-top: 8px; font-size: 25px; letter-spacing: -0.025em; }
	.docs-content :global(h3) { margin: 30px 0 10px; font-size: 17px; font-weight: 500; }
	.docs-content :global(p), .docs-content :global(li) { color: var(--muted-foreground); font-size: 15px; line-height: 1.8; }
	.docs-content :global(p) { margin-block: 14px; }
	.docs-content :global(ul), .docs-content :global(ol) { margin: 14px 0; padding-left: 24px; }
	.docs-content :global(li + li) { margin-top: 7px; }
	.docs-content :global(a) { color: #366e71; text-decoration: underline; text-decoration-color: #9ebbbc; text-underline-offset: 3px; }
	.docs-content :global(a:hover) { color: var(--primary); text-decoration-color: currentColor; }
	.docs-content :global(strong) { color: var(--foreground); font-weight: 500; }
	.docs-content :global(code) { border: 1px solid var(--border); background: var(--muted); padding: 2px 5px; color: var(--foreground); font-size: 0.85em; }
	.docs-content :global(pre) { position: relative; overflow-x: auto; margin: 20px 0; border: 1px solid #45433e; background: #282828; padding: 18px 82px 18px 20px; }
	.docs-content :global(pre code) { border: 0; background: transparent; padding: 0; color: #ebdbb2; font-size: 13px; line-height: 1.7; white-space: pre; }
	.docs-content :global(pre code .token) { display: inline; }
	.docs-content :global(pre code .token.punctuation) { color: #bdae93; }
	.docs-content :global(pre code .token.key),
	.docs-content :global(pre code .token.property) { color: #83a598; }
	.docs-content :global(pre code .token.table),
	.docs-content :global(pre code .token.class-name) { color: #fabd2f; }
	.docs-content :global(pre code .token.string) { color: #b8bb26; }
	.docs-content :global(pre code .token.boolean) { color: #d3869b; }
	.docs-content :global(pre code .token.number) { color: #fe8019; }
	.docs-content :global(pre code .token.comment) { color: #a89984; font-style: italic; }
	.docs-content :global(pre code .token.function) { color: #8ec07c; }
	.docs-content :global(pre code .token.variable),
	.docs-content :global(pre code .token.parameter) { color: #d3869b; }
	.docs-content :global(pre code .token.operator),
	.docs-content :global(pre code .token.keyword) { color: #fe8019; }
	.docs-content :global(.code-copy-button) { position: absolute; top: 9px; right: 9px; min-width: 58px; height: 32px; border: 1px solid #5d584e; background: #282828; color: #bdae93; font: 11px var(--font-mono); }
	.docs-content :global(.code-copy-button:hover) { background: #3c3836; color: #ebdbb2; }
	.docs-content :global(.code-copy-button:focus-visible) { outline-color: #8ec07c; }
	.docs-content :global(blockquote) { margin: 24px 0; border-left: 3px solid var(--primary); background: var(--surface); padding: 14px 18px; }
	.docs-content :global(blockquote p) { margin: 0; }
	.docs-content :global(table) { width: 100%; margin: 24px 0; border-collapse: collapse; font-size: 14px; line-height: 1.6; }
	.docs-content :global(th), .docs-content :global(td) { border-bottom: 1px solid var(--border); padding: 10px 12px; text-align: left; vertical-align: top; }
	.docs-content :global(th) { color: var(--heading); font-weight: 500; }
	.docs-content :global(td) { color: var(--muted-foreground); }
	.docs-content :global(hr) { margin: 40px 0; border: 0; border-top: 1px solid var(--border); }

	@media (max-width: 760px) {
		.docs-shell { grid-template-columns: 1fr; gap: 36px; padding-block: 36px 64px; }
		aside { position: static; }
		aside { max-height: none; overflow: visible; }
		aside nav { grid-template-columns: repeat(2, minmax(0, 1fr)); }
		.docs-content :global(h2) { margin-top: 40px; }
	}

	@media (max-width: 600px) {
		aside nav { grid-template-columns: 1fr; }
		.docs-content :global(p), .docs-content :global(li) { font-size: 14px; }
	}
</style>
