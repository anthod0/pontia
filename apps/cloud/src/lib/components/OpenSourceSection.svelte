<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import { reveal } from '$lib/reveal';

	const repository = 'https://github.com/anthod0/pontia';
	const components = [
		{ path: 'crates/pontiad', role: 'Device daemon' },
		{ path: 'crates/pontia-e2e', role: 'Encryption protocol' },
		{ path: 'crates/pontia-edge', role: 'Self-hostable relay' },
		{ path: 'apps/dashboard', role: 'Dashboard' },
		{ path: 'apps/cloud', role: 'Sign-in and authorization' }
	];
</script>

<section id="open-source" class="open-source page-width section-block" aria-labelledby="open-source-title" {@attach reveal()}>
	<div class="panel">
		<div class="glow" aria-hidden="true"></div>
		<div class="copy reveal">
			<h2 id="open-source-title">Every part of Pontia is open source.</h2>
			<p class="lede">The daemon, Dashboard, encryption, relay, and Cloud live in one public repository. See exactly what runs on your device.</p>
			<div class="actions">
				<a class="button" href={repository} target="_blank" rel="noreferrer">View on GitHub<ArrowUpRightIcon size={16} /></a>
				<a class="text-link" href="/docs/security">Security model<ArrowRightIcon size={12} /></a>
			</div>
		</div>

		<nav class="repo reveal" style="--i: 2" aria-label="Pontia source code">
			<div class="repo-header"><span class="lights"><i></i><i></i><i></i></span><span>anthod0/pontia</span></div>
			<ul>
				{#each components as component, index}
					<li class="reveal" style={`--i: ${index + 3}`}><a href={`${repository}/tree/main/${component.path}`} target="_blank" rel="noreferrer"><code>{component.path}</code><span>{component.role}</span><ArrowUpRightIcon size={12} /></a></li>
				{/each}
			</ul>
		</nav>
	</div>
</section>

<style>
	.panel { position: relative; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); align-items: center; gap: 40px 64px; overflow: hidden; padding: 56px; border: 1px solid #45433e; border-radius: 12px; background: #282828; color: #d5c4a1; isolation: isolate; }
	/* A slow-drifting light behind the panel content. */
	.glow { position: absolute; z-index: -1; inset: -40% -10% auto auto; width: 520px; height: 520px; border-radius: 50%; background: radial-gradient(circle, rgb(142 192 124 / 18%), transparent 65%); animation: drift 14s ease-in-out infinite alternate; }
	h2 { color: #fbf1c7; font-size: 40px; }
	.lede { margin-top: 16px; max-width: 440px; font-size: 15px; line-height: 1.75; color: #bdae93; text-wrap: pretty; }
	.actions { display: flex; flex-wrap: wrap; align-items: center; gap: 12px 24px; margin-top: 32px; }
	.actions .button { gap: 10px; background: #ebdbb2; border-color: #ebdbb2; color: #282828; font-weight: 600; }
	.actions .button:hover { background: #fbf1c7; border-color: #fbf1c7; }
	.text-link { display: inline-flex; align-items: center; gap: 4px; color: #ebdbb2; font-size: 13px; font-weight: 500; }
	.text-link:hover { text-decoration: underline; text-underline-offset: 3px; }
	.open-source a:focus-visible { outline-color: #8ec07c; }

	.repo { overflow: hidden; border: 1px solid #45433e; border-radius: 8px; background: #1d2021; box-shadow: 0 24px 48px -24px rgb(0 0 0 / 60%); }
	.repo-header { display: flex; align-items: center; gap: 14px; padding: 12px 16px; border-bottom: 1px solid #3c3836; font: 12px var(--font-mono); color: #ebdbb2; }
	.lights { display: flex; gap: 6px; }
	.lights i { width: 9px; height: 9px; border-radius: 50%; background: #504945; }
	.repo ul { margin: 0; padding: 0; list-style: none; }
	.repo li + li { border-top: 1px solid #32302f; }
	.repo a { display: flex; align-items: center; gap: 12px; padding: 13px 16px; color: #a89984; transition: background 0.15s, color 0.15s; }
	.repo a:hover { background: #282828; color: #ebdbb2; }
	.repo code { font-size: 12px; color: #8ec07c; }
	.repo a span { margin-left: auto; font-size: 12px; }
	.repo a :global(svg) { flex-shrink: 0; opacity: 0; transform: translate(-4px, 4px); transition: opacity 0.15s, transform 0.15s; }
	.repo a:hover :global(svg) { opacity: 1; transform: none; }

	@keyframes drift { to { transform: translate(-180px, 120px); } }
	@media (prefers-reduced-motion: reduce) {
		.glow { animation: none; }
	}
	@media (max-width: 900px) {
		.panel { grid-template-columns: 1fr; padding: 40px; }
	}
	@media (max-width: 600px) {
		.panel { gap: 28px; padding: 32px 20px; }
		h2 { font-size: 28px; }
		.repo a { flex-direction: column; align-items: flex-start; gap: 2px; }
		.repo a span { margin-left: 0; }
		.repo a :global(svg) { display: none; }
	}
</style>
