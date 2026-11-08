<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import AnimatedLogo from '$lib/components/AnimatedLogo.svelte';

	const dashboardScreenshot = 'https://files.pontia.dev/cloud/dashboard-1280-79de754face8.webp';
	// Install command split into tokens so commands can be coloured; the copied text is their concatenation.
	const installTokens: { text: string; cmd?: boolean }[] = [
		{ text: 'curl', cmd: true },
		{ text: ' -fsSL https://get.pontia.dev/install.sh | ' },
		{ text: 'sh', cmd: true },
		{ text: ' && ' },
		{ text: '"$HOME/.local/bin/pontia"', cmd: true },
		{ text: ' init' }
	];
	const installCommand = installTokens.map((token) => token.text).join('');
	let copyStatus = $state<'idle' | 'copied' | 'error'>('idle');
	let startHovered = $state(false);
	let mascot: AnimatedLogo;

	async function copyCommand() {
		try {
			await navigator.clipboard.writeText(installCommand);
			copyStatus = 'copied';
			mascot.poke();
		} catch {
			copyStatus = 'error';
		}
	}
</script>

<section class="hero page-width" aria-labelledby="hero-title">
	<div class="hero-copy">
		<h1 id="hero-title">Your device.<br /><span>Your agent cloud.</span></h1>
		<p class="hero-description">An open-source agent cloud on your device. Control it from anywhere.</p>
		<div class="hero-mascot" aria-hidden="true"><AnimatedLogo bind:this={mascot} class="mascot" mood={startHovered ? 'curious' : 'idle'} /></div>
		<div class="install-command">
			<code><span class="prompt">$</span>{#each installTokens as token}{#if token.cmd}<span class="cmd">{token.text}</span>{:else}{token.text}{/if}{/each}</code>
			<button onclick={copyCommand} aria-label={copyStatus === 'copied' ? 'Copy install command again' : 'Copy install command'} title="Copy install command">
				{#if copyStatus === 'copied'}<CheckIcon size={17} />{:else}<CopyIcon size={17} />{/if}
			</button>
		</div>
		<div class="install-meta">
			<span><a href="/docs/getting-started">Read the setup guide<ArrowRightIcon size={12} /></a></span>
			<span class="copy-feedback" aria-live="polite">
				{copyStatus === 'copied' ? 'Copied to clipboard.' : copyStatus === 'error' ? 'Could not copy. Select the command above to copy manually.' : ''}
			</span>
		</div>
		<div class="hero-actions">
			<a class="button" href="https://app.pontia.dev" onpointerenter={() => (startHovered = true)} onpointerleave={() => (startHovered = false)} onfocus={() => (startHovered = true)} onblur={() => (startHovered = false)}>Get started<ArrowRightIcon size={18} /></a>
			<a class="button button-secondary" href="https://github.com/anthod0/pontia" target="_blank" rel="noreferrer">
				GitHub
				<svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
					<path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"></path>
				</svg>
			</a>
		</div>
	</div>

	<figure class="dashboard-preview">
		<a class="dashboard-image" href={dashboardScreenshot} target="_blank" rel="noreferrer" aria-label="View full-size Dashboard screenshot (opens in a new tab)">
			<img
				src={dashboardScreenshot}
				width="1280"
				height="960"
				alt="Pontia Dashboard showing recent sessions, a coding conversation, tool execution, test results, and the message composer."
				fetchpriority="high"
			/>
		</a>
	</figure>

	<hr class="hero-divider" />
</section>

<style>
	.hero { padding-top: 64px; }
	/* The mascot stands on the install command bar, beside the headline. */
	.hero-copy { display: grid; grid-template-columns: minmax(0, 1fr) auto; column-gap: 32px; padding-bottom: 40px; }
	h1 { grid-column: 1; font-size: clamp(42px, 5.6vw, 68px); font-weight: 500; line-height: 1.04; letter-spacing: -.05em; margin: 0 0 22px; }
	h1 > span { color: var(--primary); }
	.hero-description { grid-column: 1; max-width: 520px; font-size: 16px; line-height: 1.7; color: var(--muted-foreground); }
	.hero-mascot { grid-column: 2; grid-row: 1 / span 2; align-self: end; width: 232px; margin: 0 28px -84px 0; }
	.hero-mascot :global(.mascot) { width: 100%; height: auto; }
	.install-command { position: relative; grid-column: 1 / -1; display: flex; align-items: center; margin-top: 28px; padding: 7px 7px 7px 18px; background: #282828; border: 1px solid #45433e; border-radius: 6px; color: #ebdbb2; }
	.install-command code { min-width: 0; padding: 6px 16px 6px 0; overflow-wrap: anywhere; font-size: 13px; line-height: 1.6; white-space: normal; }
	.install-command .prompt { margin-right: 10px; color: #928374; user-select: none; }
	.install-command .cmd { color: #8ec07c; }
	.install-command button { display: flex; flex: 0 0 34px; align-items: center; justify-content: center; width: 34px; height: 34px; margin-left: auto; border: 1px solid #5d584e; border-radius: 4px; background: transparent; color: #bdae93; }
	.install-command button:hover { color: #ebdbb2; background: #3c3836; }
	.install-command button:focus-visible { outline-color: #8ec07c; }
	.install-meta { grid-column: 1 / -1; display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 16px; margin-top: 10px; font-size: 13px; color: var(--muted-foreground); }
	.install-meta a { display: inline-flex; align-items: center; gap: 4px; color: var(--foreground); font-weight: 500; text-decoration: underline; text-decoration-color: var(--border); text-underline-offset: 3px; }
	.install-meta a:hover { text-decoration-color: currentColor; }
	.copy-feedback { color: #6b7620; font: 11px/1.8 var(--font-mono); }
	.hero-actions { grid-column: 1 / -1; display: flex; flex-wrap: wrap; gap: 12px; margin-top: 24px; }
	.dashboard-preview { margin: 0 0 48px; }

	/* Entrance: the mascot pops up from behind the command bar. */
	.hero-mascot { transform-origin: 50% 100%; animation: pop 0.75s cubic-bezier(0.3, 0.7, 0.4, 1) 200ms both; }
	@keyframes pop {
		0% { opacity: 0; transform: translateY(18px) scale(0.8, 0.6); }
		55% { opacity: 1; transform: translateY(-6px) scale(0.96, 1.08); }
		80% { transform: scale(1.03, 0.96); }
		100% { transform: none; }
	}
	@media (prefers-reduced-motion: reduce) {
		.hero-mascot { animation: none; }
	}
	.dashboard-image { display: block; overflow: hidden; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); box-shadow: 0 16px 48px -16px rgb(60 56 54 / 16%); }
	.dashboard-image img { display: block; width: 100%; height: auto; }
	.hero-divider { margin: 0; border: 0; border-top: 1px solid var(--border); }

	@media (max-width: 760px) {
		.hero { padding-top: 40px; }
		.hero-copy { column-gap: 12px; padding-bottom: 36px; }
		/* The headline takes the full width; the mascot moves beside the description. */
		h1 { grid-column: 1 / -1; }
		.hero-mascot { grid-row: 2; width: 100px; margin: 0 8px -46px 0; }
		.dashboard-preview { margin-bottom: 32px; }
	}
	@media (max-width: 600px) {
		h1 { font-size: clamp(30px, 9.4vw, 42px); }
		.hero-description { font-size: 14px; }
		.install-command { margin-top: 22px; padding-left: 14px; }
		.install-command code { font-size: 11px; }
		.hero-actions { gap: 9px; }
		.hero-actions :global(.button) { min-height: 43px; padding-inline: 15px; font-size: 12px; gap: 10px; }
	}
</style>
