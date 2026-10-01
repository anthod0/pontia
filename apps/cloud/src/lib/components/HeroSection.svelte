<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import GithubLogoIcon from 'phosphor-svelte/lib/GithubLogoIcon';

	let { repository }: { repository: string } = $props();
</script>

<section class="hero page-width" aria-labelledby="hero-title">
	<div class="hero-copy">
		<h1 id="hero-title">Keep work moving.<br /><span>Wherever you are.</span></h1>
		<p class="hero-description">
			Run agents on your machine. Start work, follow progress, and step in from wherever you are—without spending your day managing terminal sessions.
		</p>
		<div class="hero-actions">
			<a class="button" href="#get-started">Get started<ArrowRightIcon size={17} /></a>
			<a class="button button-secondary" href={repository}><GithubLogoIcon size={18} />View on GitHub</a>
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
	.hero-actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 12px; margin: 29px 0 17px; }
	.hero-divider { margin: 0; border: 0; border-top: 1px solid var(--border); }

	@media (max-width: 760px) {
		.hero { padding-top: 52px; }
		.hero-copy { padding-bottom: 50px; }
	}
	@media (max-width: 600px) {
		h1 { margin-block: 23px 20px; }
		.hero-description { font-size: 14px; }
		.hero-actions { margin-top: 24px; gap: 9px; }
		.hero-actions :global(.button) { min-height: 43px; padding-inline: 15px; font-size: 12px; gap: 10px; }
	}
</style>
