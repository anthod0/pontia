<script lang="ts">
	import { onMount } from 'svelte';
	import { reveal } from '$lib/reveal';
	import { m } from '$lib/paraglide/messages';

	type Unit = {
		client: string;
		logo: string;
		darkLogo?: string;
		title: string;
		workspace: string;
		branch: string;
		model: string;
		state: 'busy' | 'waiting' | 'idle';
		/** Current step for busy units, pending request for waiting units. */
		activity?: string;
		/** Seconds the current Turn has been running. */
		elapsed?: number;
	};

	// SVG logo sources: https://pi.dev/press-kit; Claude symbol extracted from
	// the inline wordmark at https://claude.com/product/claude-code;
	// https://grok.com/images/favicon.svg; OpenCode brand assets at
	// https://opencode.ai/brand. The OpenAI symbol used for Codex comes from
	// anomalyco/opencode: packages/ui/src/assets/icons/provider/openai.svg.
	// Local vector assets live in static/client-logos/.
	// Sample Sessions; each one is an isolated unit with its own Workspace, branch, model, and state.
	const units: Unit[] = [
		{ client: 'Codex', logo: '/client-logos/codex.svg', darkLogo: '/client-logos/codex-dark.svg', title: 'Add keyboard shortcuts', workspace: '~/code/atlas', branch: 'feat/keyboard-shortcuts', model: 'gpt-5.4', state: 'busy', activity: 'Running bun test', elapsed: 83 },
		{ client: 'Claude Code', logo: '/client-logos/claude-code.svg', title: 'Review API changes', workspace: '~/code/atlas', branch: 'main', model: 'claude-sonnet-4.5', state: 'waiting', activity: 'git diff v1.8..HEAD' },
		{ client: 'Pi', logo: '/client-logos/pi.svg', title: 'Add integration tests', workspace: '~/code/atlas', branch: 'test/checkout-flow', model: 'gpt-5.4', state: 'busy', activity: 'Editing tests/checkout.spec.ts', elapsed: 214 },
		{ client: 'Grok Build', logo: '/client-logos/grok-build.svg', title: 'Investigate flaky CI job', workspace: '~/code/infra', branch: 'main', model: 'grok-code-fast-1', state: 'busy', activity: 'Reading 6 files', elapsed: 41 },
		{ client: 'OpenCode', logo: '/client-logos/opencode.svg', darkLogo: '/client-logos/opencode-dark.svg', title: 'Build the settings page', workspace: '~/code/atlas', branch: 'feat/settings', model: 'kimi-k2', state: 'idle' },
		{ client: 'Codex', logo: '/client-logos/codex.svg', darkLogo: '/client-logos/codex-dark.svg', title: 'Rotate staging certificates', workspace: '~/code/infra', branch: 'ops/certs', model: 'gpt-5.4-mini', state: 'idle' }
	];
	const stateLabel = { busy: m.features_state_busy, waiting: m.features_state_waiting, idle: m.features_state_idle };

	// Busy units count up while the section is on screen.
	let tick = $state(0);
	onMount(() => {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		const timer = setInterval(() => tick++, 1000);
		return () => clearInterval(timer);
	});
	const clock = (seconds: number) => `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
</script>

<section id="features" class="units-section page-width section-block" aria-labelledby="features-title" {@attach reveal()}>
	<div class="section-heading reveal">
		<h2 id="features-title">{m.features_title()}</h2>
		<p>{m.features_intro()}</p>
	</div>

	<ul class="units" aria-hidden="true">
		{#each units as unit, index}
			<li class={['unit', 'reveal', unit.state]} style={`--i: ${index % 3}`}>
				<div class="unit-head">
					<span class="client">
						<img class:light-logo={unit.darkLogo} src={unit.logo} alt="" width="16" height="16" loading="lazy" />
						{#if unit.darkLogo}<img class="dark-logo" src={unit.darkLogo} alt="" width="16" height="16" loading="lazy" />{/if}
						{unit.client}
					</span>
					<span class="state"><span class="dot"></span>{stateLabel[unit.state]()}</span>
				</div>
				<p class="unit-title">{unit.title}</p>
				<dl class="unit-meta">
					<dt>cwd</dt><dd>{unit.workspace}</dd>
					<dt>branch</dt><dd>{unit.branch}</dd>
					<dt>model</dt><dd>{unit.model}</dd>
				</dl>
				<div class="unit-foot">
					{#if unit.state === 'busy'}
						<span class="activity">› {unit.activity}</span><span class="elapsed">{clock(unit.elapsed! + tick)}</span>
					{:else if unit.state === 'waiting'}
						<span class="activity">$ {unit.activity}</span><span class="approve">{m.features_approve()}</span>
					{:else}
						<span class="activity muted">—</span>
					{/if}
				</div>
			</li>
		{/each}
	</ul>

</section>

<style>
	.section-heading { margin-bottom: 35px; }

	/* One hairline grid: units sit side by side as equal, separate cells. */
	.units { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 1px; margin: 0; padding: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 10px; background: var(--border); list-style: none; }
	.unit { display: flex; flex-direction: column; gap: 14px; min-width: 0; padding: 20px 22px; background: var(--surface); }
	.unit-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
	.client { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 500; color: var(--heading); }
	.client img { width: 16px; height: 16px; object-fit: contain; }
	.client img.dark-logo { display: none; }
	:global(.dark) .client img.light-logo { display: none; }
	:global(.dark) .client img.dark-logo { display: block; }
	.state { display: inline-flex; align-items: center; gap: 6px; font: 11px var(--font-mono); color: var(--muted-foreground); }
	.dot { width: 6px; height: 6px; border-radius: 50%; background: var(--color-idle); }
	.busy .dot { background: var(--color-busy); animation: pulse 1.6s ease-out infinite; }
	.waiting .dot { background: var(--color-interrupted); }
	.waiting .state { color: var(--color-interrupted); }
	.unit-title { font-size: 16px; font-weight: 500; letter-spacing: -0.015em; color: var(--heading); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.unit-meta { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 4px 14px; margin: 0; font: 11px/1.5 var(--font-mono); }
	.unit-meta dt { color: var(--inactive); }
	.unit-meta dd { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--foreground); }
	.unit-foot { display: flex; align-items: center; justify-content: space-between; gap: 12px; height: 26px; padding-top: 12px; border-top: 1px dashed var(--border); font: 11px var(--font-mono); box-sizing: content-box; }
	.activity { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--foreground); }
	.busy .activity { color: var(--color-busy); }
	.activity.muted { color: var(--inactive); }
	.elapsed { flex-shrink: 0; color: var(--muted-foreground); font-variant-numeric: tabular-nums; }
	.approve { flex-shrink: 0; padding: 3px 8px; border-radius: 4px; background: var(--button); color: white; font: 500 11px var(--font-sans); }


	@keyframes pulse { 0% { box-shadow: 0 0 0 0 rgb(215 153 33 / 50%); } 100% { box-shadow: 0 0 0 6px rgb(215 153 33 / 0%); } }
	@media (prefers-reduced-motion: reduce) {
		.busy .dot { animation: none; }
	}
	@media (max-width: 900px) {
		.units { grid-template-columns: repeat(2, minmax(0, 1fr)); }
	}
	@media (max-width: 600px) {
		.units { grid-template-columns: 1fr; }
		.unit:nth-child(n + 4) { display: none; }
		.unit { padding: 18px; }
	}
</style>
