<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import RocketLaunchIcon from 'phosphor-svelte/lib/RocketLaunchIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';

	const installCommand = 'curl -fsSL https://get.pontia.dev/install.sh | sh';
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
		<p class="hero-description">Run agents on your device. Control them from anywhere.</p>
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
			<a class="button button-secondary" href="https://app.pontia.dev">Open Dashboard<RocketLaunchIcon size={18} /></a>
		</div>
	</div>

	<hr class="hero-divider" />
</section>

<style>
	.hero { padding-top: 80px; }
	.hero-copy { max-width: 820px; margin-inline: auto; padding-bottom: 75px; text-align: center; }
	h1 { font-size: clamp(42px, 6.5vw, 78px); font-weight: 500; line-height: 1.08; letter-spacing: -.055em; margin: 24px 0; }
	h1 > span { color: var(--primary); }
	.hero-description { max-width: 500px; margin-inline: auto; font-size: 16px; line-height: 1.8; color: var(--muted-foreground); }
	.install-command { display: flex; align-items: center; width: min(620px, 100%); margin: 30px auto 0; padding: 7px 7px 7px 20px; background: #282828; border: 1px solid #45433e; color: #ebdbb2; text-align: left; }
	.install-command code { min-width: 0; overflow-x: auto; padding-right: 16px; font-size: 13px; line-height: 34px; white-space: nowrap; }
	.install-command button { display: flex; flex: 0 0 34px; align-items: center; justify-content: center; width: 34px; height: 34px; margin-left: auto; border: 1px solid #5d584e; background: transparent; color: #bdae93; }
	.install-command button:hover { color: #ebdbb2; background: #3c3836; }
	.install-command button:focus-visible { outline-color: #8ec07c; }
	.copy-feedback { min-height: 17px; margin-top: 7px; color: #6b7620; font: 10px/1.6 var(--font-mono); }
	.hero-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin-top: 13px; }
	.hero-divider { margin: 0; border: 0; border-top: 1px solid var(--border); }

	@media (max-width: 760px) {
		.hero { padding-top: 52px; }
		.hero-copy { padding-bottom: 50px; }
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
