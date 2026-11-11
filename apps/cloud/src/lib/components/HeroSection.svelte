<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import CopyIcon from 'phosphor-svelte/lib/CopyIcon';
	import LockSimpleIcon from 'phosphor-svelte/lib/LockSimpleIcon';
	import DashboardDemo from '$lib/components/dashboard-demo/DashboardDemo.svelte';

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

	async function copyCommand() {
		try {
			await navigator.clipboard.writeText(installCommand);
			copyStatus = 'copied';
		} catch {
			copyStatus = 'error';
		}
	}
</script>

<section class="hero" aria-labelledby="hero-title">
	<div class="backdrop" aria-hidden="true"></div>

	<div class="hero-copy page-width">
		<a class="badge enter" href="https://github.com/anthod0/pontia" target="_blank" rel="noreferrer">
			<span class="badge-dot"></span>Open source · Apache-2.0<ArrowRightIcon size={12} />
		</a>
		<h1 id="hero-title">
			<span class="line enter" style="--d: 1">Your device.</span>
			<span class="line accent enter" style="--d: 2">Your agent cloud.</span>
		</h1>
		<p class="hero-description enter" style="--d: 3">Leave your desk, not your agents. Check in, approve, and steer every session, everywhere.</p>

		<div class="hero-actions enter" style="--d: 4">
			<a class="button" href="https://app.pontia.dev">Get started<ArrowRightIcon size={18} /></a>
			<a class="button button-secondary" href="https://github.com/anthod0/pontia" target="_blank" rel="noreferrer">
				<svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
					<path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"></path>
				</svg>
				GitHub
			</a>
		</div>

		<div class="install enter" style="--d: 5">
			<div class="install-command">
				<code><span class="prompt">$</span>{#each installTokens as token}{#if token.cmd}<span class="cmd">{token.text}</span>{:else}{token.text}{/if}{/each}</code>
				<button onclick={copyCommand} aria-label={copyStatus === 'copied' ? 'Copy install command again' : 'Copy install command'} title="Copy install command">
					{#if copyStatus === 'copied'}<CheckIcon size={16} />{:else}<CopyIcon size={16} />{/if}
				</button>
			</div>
			<p class="install-meta">
				<span class="copy-feedback" aria-live="polite">{copyStatus === 'copied' ? 'Copied to clipboard.' : copyStatus === 'error' ? 'Could not copy. Select the command to copy it.' : 'Linux and macOS'}</span>
				<a href="/docs/getting-started">Setup guide<ArrowRightIcon size={12} /></a>
			</p>
		</div>
	</div>

	<figure class="dashboard-preview page-width enter" style="--d: 6">
		<div class="window">
			<div class="window-bar" aria-hidden="true">
				<span class="lights"><i></i><i></i><i></i></span>
				<span class="address"><LockSimpleIcon size={11} weight="bold" />app.pontia.dev/studio</span>
			</div>
			<div class="dashboard-frame"><DashboardDemo /></div>
		</div>
		<figcaption class="sr-only">Interactive Dashboard demo with sample data. Select a session or switch devices to explore.</figcaption>
	</figure>
</section>

<style>
	.hero { position: relative; overflow: hidden; padding-top: 72px; }

	/* Dotted grid with a teal glow, fading out toward the demo. */
	.backdrop { position: absolute; z-index: -1; inset: 0 0 auto; height: 900px; pointer-events: none; background: radial-gradient(ellipse 45% 40% at 50% 30%, rgb(69 133 136 / 12%), transparent 70%), radial-gradient(circle at 1px 1px, #dedbd4 1px, transparent 0) 0 0 / 22px 22px; mask-image: radial-gradient(ellipse 70% 65% at 50% 25%, #000 30%, transparent 75%); }

	.hero-copy { display: flex; flex-direction: column; align-items: center; text-align: center; }
	.badge { display: inline-flex; align-items: center; gap: 8px; margin-bottom: 28px; padding: 5px 12px 5px 10px; border: 1px solid var(--border); border-radius: 999px; background: rgb(255 254 252 / 80%); backdrop-filter: blur(6px); font: 12px/1.5 var(--font-mono); color: var(--muted-foreground); transition: border-color 0.2s, color 0.2s; }
	.badge:hover { border-color: #c9c5bc; color: var(--heading); }
	.badge :global(svg) { transition: transform 0.2s var(--ease-out); }
	.badge:hover :global(svg) { transform: translateX(2px); }
	.badge-dot { width: 6px; height: 6px; border-radius: 50%; background: #79740e; box-shadow: 0 0 0 3px rgb(121 116 14 / 15%); }

	h1 { display: flex; flex-direction: column; font-size: clamp(44px, 7.4vw, 92px); font-weight: 500; line-height: 1; letter-spacing: -0.055em; }
	.accent { padding-bottom: 0.08em; background: linear-gradient(100deg, #2f6f72 10%, #458588 45%, #6aa19a 60%, #458588 75%); background-size: 220% 100%; background-clip: text; -webkit-background-clip: text; color: transparent; }
	.hero-description { max-width: 520px; margin-top: 24px; font-size: 18px; line-height: 1.6; color: var(--muted-foreground); text-wrap: balance; }
	.hero-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin-top: 36px; }
	.hero-actions .button-secondary { background: var(--background); }
	.hero-actions .button :global(svg) { transition: transform 0.2s var(--ease-out); }
	.hero-actions .button:not(.button-secondary):hover :global(svg) { transform: translateX(3px); }

	.install { width: min(100%, 760px); margin-top: 28px; }
	.install-command { display: flex; align-items: center; padding: 6px 6px 6px 16px; background: #282828; border: 1px solid #45433e; border-radius: 8px; color: #ebdbb2; text-align: left; box-shadow: 0 10px 30px -18px rgb(40 40 40 / 60%); }
	.install-command code { min-width: 0; padding: 6px 12px 6px 0; overflow-wrap: anywhere; font-size: 12.5px; line-height: 1.6; }
	.install-command .prompt { margin-right: 10px; color: #928374; user-select: none; }
	.install-command .cmd { color: #8ec07c; }
	.install-command button { display: flex; flex: 0 0 32px; align-items: center; justify-content: center; width: 32px; height: 32px; margin-left: auto; border: 1px solid #5d584e; border-radius: 5px; background: transparent; color: #bdae93; transition: background 0.15s, color 0.15s; }
	.install-command button:hover { color: #ebdbb2; background: #3c3836; }
	.install-command button:focus-visible { outline-color: #8ec07c; }
	.install-meta { display: flex; justify-content: space-between; gap: 16px; margin-top: 10px; padding-inline: 2px; font-size: 12px; color: var(--muted-foreground); }
	.install-meta a { display: inline-flex; align-items: center; gap: 4px; color: var(--foreground); font-weight: 500; }
	.install-meta a:hover { color: var(--primary); }
	.copy-feedback { font: 11px/1.8 var(--font-mono); }

	.dashboard-preview { margin: 88px auto 0; padding-bottom: 96px; }
	.window { position: relative; overflow: hidden; border: 1px solid #d6d3cc; border-radius: 12px; background: var(--surface); box-shadow: 0 1px 0 rgb(255 255 255 / 80%) inset, 0 40px 80px -40px rgb(40 60 60 / 35%), 0 12px 24px -12px rgb(60 56 54 / 12%); }
	.window-bar { display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; height: 40px; padding-inline: 14px; border-bottom: 1px solid var(--border); background: var(--muted); }
	.lights { display: flex; gap: 7px; }
	.lights i { width: 10px; height: 10px; border-radius: 50%; background: #dcd8d0; }
	.address { display: inline-flex; align-items: center; gap: 6px; padding: 4px 14px; border-radius: 6px; background: var(--background); font: 11px var(--font-mono); color: var(--muted-foreground); }
	.address :global(svg) { color: #79740e; }
	.dashboard-frame { height: 640px; overflow: hidden; }

	/* Load sequence: copy rises in line by line, then the demo window.
	   `backwards` fill leaves no transform or filter behind, so text renders crisply afterwards. */
	.enter { animation: rise 0.9s var(--ease-out) backwards; animation-delay: calc(var(--d, 0) * 90ms); }
	.accent { animation: rise 0.9s var(--ease-out) backwards, sheen 6s ease-in-out 1.2s infinite; animation-delay: 180ms, 1.2s; }
	@keyframes rise { from { opacity: 0; transform: translateY(20px); filter: blur(6px); } to { opacity: 1; transform: none; filter: none; } }
	@keyframes sheen { 0%, 100% { background-position: 100% 0; } 50% { background-position: 0 0; } }

	@media (prefers-reduced-motion: reduce) {
		.enter, .accent { animation: none; }
	}
	@media (max-width: 760px) {
		.hero { padding-top: 48px; }
		.hero-description { font-size: 16px; }
		.dashboard-preview { margin-top: 72px; padding-bottom: 64px; }
		.dashboard-frame { height: 560px; }
		.address { display: none; }
	}
	@media (max-width: 600px) {
		h1 { font-size: clamp(36px, 11vw, 48px); }
		.badge { margin-bottom: 22px; font-size: 11px; }
		.hero-description { font-size: 15px; }
		.hero-actions { gap: 9px; margin-top: 28px; }
		.hero-actions :global(.button) { min-height: 43px; padding-inline: 16px; font-size: 13px; gap: 10px; }
		.install-command { padding-left: 12px; }
		.install-command code { font-size: 11px; }
		/* On phones the demo renders inside a phone frame instead of a browser window. */
		.window { width: min(82%, 300px); margin-inline: auto; border: 4px solid #282828; border-radius: 34px; box-shadow: 0 0 0 1px #45433e, 0 40px 80px -40px rgb(40 60 60 / 45%), 0 12px 24px -12px rgb(60 56 54 / 18%); }
		.window-bar { display: none; }
		.dashboard-frame { height: min(540px, 130vw); }
	}
</style>
