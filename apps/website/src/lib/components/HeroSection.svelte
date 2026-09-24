<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import GithubLogoIcon from 'phosphor-svelte/lib/GithubLogoIcon';

	let { repository }: { repository: string } = $props();
</script>

<section class="hero page-width" aria-labelledby="hero-title">
	<div class="hero-copy">
		<p class="eyebrow"><span class="status-dot"></span>A HOME FOR YOUR CODING AGENTS</p>
		<h1 id="hero-title">Beyond the terminal.<br /><span>Still working.</span></h1>
		<p class="hero-description">
			Close the window. Not the session. Keep your coding agents running, and pick up in the terminal or the browser. Same agent. Same context.
		</p>
		<div class="hero-actions">
			<a class="button" href="#get-started">Get started<ArrowRightIcon size={17} /></a>
			<a class="button button-secondary" href={repository}><GithubLogoIcon size={18} />View on GitHub</a>
		</div>
		<p class="hero-note">Built for pi <span>·</span> Linux & macOS <span>·</span> Experimental</p>
	</div>

	<hr class="hero-divider" />
</section>

<style>
	.hero { padding-top: 80px; }
	.hero-copy { max-width: 820px; margin-inline: auto; padding-bottom: 75px; text-align: center; }
	.eyebrow { font-size: 9px; letter-spacing: .09em; }
	h1 { font-size: clamp(42px, 6.5vw, 78px); font-weight: 500; line-height: 1.08; letter-spacing: -.055em; margin: 24px 0; }
	h1 > span { color: var(--primary); }
	.hero-description { max-width: 500px; margin-inline: auto; font-size: 16px; line-height: 1.8; color: var(--muted-foreground); }
	.hero-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin: 29px 0 17px; }
	.hero-note { font: 9px/1.8 var(--font-mono); color: var(--muted-foreground); }
	.hero-note > span { margin-inline: 7px; color: #afa69b; }
	.hero-divider { margin: 0; border: 0; border-top: 1px solid var(--border); }

	@media (max-width: 760px) {
		.hero { padding-top: 52px; }
		.hero-copy { padding-bottom: 50px; }
	}
	@media (max-width: 600px) {
		.eyebrow { font-size: 8px; letter-spacing: .05em; }
		h1 { margin-block: 23px 20px; }
		.hero-description { font-size: 14px; }
		.hero-actions { margin-top: 24px; gap: 9px; }
		.hero-actions :global(.button) { min-height: 43px; padding-inline: 15px; font-size: 12px; gap: 10px; }
		.hero-note { font-size: 8px; }
		.hero-note > span { margin-inline: 4px; }
	}
</style>
