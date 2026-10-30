<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';

	const dashboardScreenshot = 'https://files.pontia.dev/cloud/dashboard-1280-647254d2c10e.webp';
	const installCommand = 'curl -fsSL https://get.pontia.dev/install.sh | sh && "$HOME/.local/bin/pontia" init';
	let copyStatus = $state<'idle' | 'copied' | 'error'>('idle');

	async function copyCommand() {
		try {
			await navigator.clipboard.writeText(installCommand);
			copyStatus = 'copied';
		} catch {
			copyStatus = 'error';
		}
	}
</script>

<section class="hero page-width" aria-labelledby="hero-title">
	<div class="hero-copy">
		<h1 id="hero-title">Your device.<br /><span>Your agent cloud.</span></h1>
		<p class="hero-description">An open-source agent cloud on your device. Control it from anywhere.</p>
		<div class="install-command">
			<code>{installCommand}</code>
			<button onclick={copyCommand} aria-label={copyStatus === 'copied' ? 'Copy install command again' : 'Copy install command'} title="Copy install command">
				{#if copyStatus === 'copied'}<CheckIcon size={17} />{:else}<CopyIcon size={17} />{/if}
			</button>
		</div>
		<p class="copy-feedback" aria-live="polite">
			{copyStatus === 'copied' ? 'Copied to clipboard.' : copyStatus === 'error' ? 'Could not copy. Select the command above to copy manually.' : ''}
		</p>
		<div class="hero-actions">
			<a class="button" href="/docs/getting-started">Get started<ArrowRightIcon size={18} /></a>
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
				height="840"
				alt="Pontia Dashboard showing recent sessions, a coding conversation, tool execution, test results, and the message composer."
				fetchpriority="high"
			/>
		</a>
	</figure>

	<hr class="hero-divider" />
</section>

<style>
	.hero { padding-top: 80px; }
	.hero-copy { max-width: 820px; margin-inline: auto; padding-bottom: 52px; text-align: center; }
	h1 { font-size: clamp(42px, 6.5vw, 78px); font-weight: 500; line-height: 1.08; letter-spacing: -.055em; margin: 24px 0; }
	h1 > span { color: var(--primary); }
	.hero-description { max-width: 500px; margin-inline: auto; font-size: 16px; line-height: 1.8; color: var(--muted-foreground); }
	.install-command { display: flex; align-items: center; width: min(620px, 100%); margin: 30px auto 0; padding: 7px 7px 7px 20px; background: #282828; border: 1px solid #45433e; color: #ebdbb2; text-align: left; }
	.install-command code { min-width: 0; padding: 6px 16px 6px 0; overflow-wrap: anywhere; font-size: 13px; line-height: 1.6; white-space: normal; }
	.install-command button { display: flex; flex: 0 0 34px; align-items: center; justify-content: center; width: 34px; height: 34px; margin-left: auto; border: 1px solid #5d584e; background: transparent; color: #bdae93; }
	.install-command button:hover { color: #ebdbb2; background: #3c3836; }
	.install-command button:focus-visible { outline-color: #8ec07c; }
	.copy-feedback { min-height: 17px; margin-top: 7px; color: #6b7620; font: 10px/1.6 var(--font-mono); }
	.hero-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin-top: 13px; }
	.dashboard-preview { margin: 0 0 48px; }
	.dashboard-image { display: block; overflow: hidden; border: 1px solid var(--border); background: var(--surface); box-shadow: 0 16px 48px -16px rgb(60 56 54 / 16%); }
	.dashboard-image img { display: block; width: 100%; height: auto; }
	.hero-divider { margin: 0; border: 0; border-top: 1px solid var(--border); }

	@media (max-width: 760px) {
		.hero { padding-top: 52px; }
		.hero-copy { padding-bottom: 36px; }
		.dashboard-preview { margin-bottom: 32px; }
	}
	@media (max-width: 600px) {
		h1 { margin-block: 23px 20px; }
		.hero-description { font-size: 14px; }
		.install-command { margin-top: 24px; padding-left: 14px; }
		.install-command code { font-size: 11px; }
		.hero-actions { gap: 9px; }
		.hero-actions :global(.button) { min-height: 43px; padding-inline: 15px; font-size: 12px; gap: 10px; }
	}
</style>
