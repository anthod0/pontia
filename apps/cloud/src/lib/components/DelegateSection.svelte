<script lang="ts">
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import { reveal } from '$lib/reveal';
	import { m } from '$lib/paraglide/messages';

	// The classic Bot shape and colour from apps/dashboard/src/components/bots/avatar.ts.
	const botShape = 'M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z';

	// Stage coordinates (viewBox 0 0 1080 360): one wire from the Bot to each Session.
	const wires = ['M470 180 C 560 180, 580 70, 680 70', 'M470 180 L 680 180', 'M470 180 C 560 180, 580 290, 680 290'];
	const sessions = [
		{ client: 'Codex', logo: '/client-logos/codex.svg', darkLogo: '/client-logos/codex-dark.svg', title: m.delegate_task_ui },
		{ client: 'Claude Code', logo: '/client-logos/claude-code.svg', title: m.delegate_task_api },
		{ client: 'Pi', logo: '/client-logos/pi.svg', title: m.delegate_task_tests }
	];
</script>

<section id="delegate" class="page-width section-block" aria-labelledby="delegate-title" {@attach reveal()}>
	<div class="section-heading reveal">
		<h2 id="delegate-title">{m.delegate_title()}</h2>
		<p>{m.delegate_intro()}</p>
	</div>

	<!-- One loop: your message reaches the Bot, it fans out to three Sessions, and one report comes back. -->
	<div class="stage reveal" style="--i: 1" aria-hidden="true">
		<div class="you">
			<span class="bubble user">{m.delegate_user()}</span>
			<span class="bubble bot-reply plan">{m.delegate_plan()}</span>
			<span class="bubble bot-reply report"><CheckIcon size={12} weight="bold" />{m.delegate_report()}</span>
		</div>

		<svg class="wires" viewBox="0 0 1080 360" preserveAspectRatio="none">
			<path class="inbound" d="M330 180 L 380 180" />
			{#each wires as wire, index}
				<path d={wire} />
				<circle class="packet" r="3.5"><animateMotion dur="9s" begin={`${0.9 + index * 0.25}s`} repeatCount="indefinite" path={wire} keyPoints="0;1;1" keyTimes="0;0.1;1" calcMode="linear" /></circle>
			{/each}
		</svg>

		<div class="bot">
			<svg viewBox="9.9 12.4 83 83">
				<path d={botShape} fill="#458588" />
				<g fill="#fff"><rect x="33" y="45" width="10" height="18" rx="5" /><rect x="57" y="45" width="10" height="18" rx="5" /></g>
			</svg>
			<span class="halo"></span>
		</div>

		<ul class="sessions">
			{#each sessions as session, index}
				<li class="session" style={`--s: ${index}`}>
					<span class="client">
						<img class:light-logo={session.darkLogo} src={session.logo} alt="" width="14" height="14" loading="lazy" />
						{#if session.darkLogo}<img class="dark-logo" src={session.darkLogo} alt="" width="14" height="14" loading="lazy" />{/if}
						{session.client}
					</span>
					<span class="title">{session.title()}</span>
					<span class="state">
						<span class="running"><span class="dot"></span>{m.delegate_state_running()}</span>
						<span class="done"><CheckIcon size={11} weight="bold" />{m.delegate_state_done()}</span>
					</span>
				</li>
			{/each}
		</ul>
	</div>
</section>

<style>
	.section-heading { margin-bottom: 48px; }
	/* Chinese copy wraps at punctuation and spaces, keeping each phrase on one line. */
	:global(:root:lang(zh-Hans)) .section-heading :is(h2, p) { word-break: keep-all; }
	/* Japanese copy wraps between phrases instead of inside words. */
	:global(:root:lang(ja)) :is(.section-heading :is(h2, p), .bubble) { word-break: auto-phrase; }

	/* A fixed-ratio stage sized in em, so the composition scales as a whole. */
	.stage { position: relative; container-type: inline-size; aspect-ratio: 1080 / 360; padding: 0; border: 1px solid var(--border); border-radius: 14px; background: radial-gradient(ellipse 40% 60% at 40% 50%, rgb(69 133 136 / 9%), transparent 70%), radial-gradient(circle at 1px 1px, var(--border) 1px, transparent 0) 0 0 / 22px 22px, var(--surface); overflow: hidden; }
	.stage > * { position: absolute; font-size: min(16px, 1.48cqw); }

	/* Left: the only conversation the user has. */
	.you { top: 50%; left: 4%; display: flex; flex-direction: column; gap: 0.7em; width: 26%; translate: 0 -50%; }
	.bubble { width: fit-content; max-width: 100%; padding: 0.65em 1em; border-radius: 1.1em; font-size: 0.85em; line-height: 1.45; }
	.user { align-self: flex-end; border-bottom-right-radius: 0.3em; background: var(--button); color: white; }
	.bot-reply { display: inline-flex; align-items: center; gap: 0.5em; border: 1px solid var(--border); border-bottom-left-radius: 0.3em; background: var(--background); color: var(--foreground); }
	.report { border-color: color-mix(in srgb, var(--coverage-supported) 45%, var(--border)); }
	.report :global(svg) { flex-shrink: 0; color: var(--coverage-supported); }

	.wires { inset: 0; width: 100%; height: 100%; overflow: visible; }
	.wires path { fill: none; stroke: var(--border-stronger); stroke-width: 1.2; stroke-dasharray: 4 5; vector-effect: non-scaling-stroke; }
	.wires .inbound { stroke: var(--primary); }
	.packet { fill: var(--primary); }

	/* Centre: the Bot that holds every Session. */
	.bot { top: 50%; left: 40.3%; width: 7.4em; height: 7.4em; translate: -50% -50%; }
	.bot svg { position: relative; z-index: 1; width: 100%; height: 100%; overflow: visible; animation: bob 4.2s ease-in-out infinite; }
	.halo { position: absolute; inset: -0.9em; border: 1px solid color-mix(in srgb, var(--primary) 35%, transparent); border-radius: 50%; background: var(--background); box-shadow: 0 0 0 0.7em color-mix(in srgb, var(--primary) 7%, transparent); }

	/* Right: the Sessions the Bot opened, each in its own Agent Client. */
	.sessions { top: 0; right: 4%; bottom: 0; display: flex; flex-direction: column; justify-content: space-between; width: 33%; margin: 0; padding: 2.3em 0; list-style: none; }
	.session { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 0.3em 1em; padding: 0.85em 1.1em; border: 1px solid var(--border); border-radius: 0.7em; background: var(--background); box-shadow: 0 1em 2em -1.6em rgb(60 56 54 / 35%); }
	.client { display: inline-flex; align-items: center; gap: 0.5em; font: 0.7em var(--font-mono); color: var(--muted-foreground); }
	.client img { width: 1.25em; height: 1.25em; object-fit: contain; }
	.client img.dark-logo { display: none; }
	:global(.dark) .client img.light-logo { display: none; }
	:global(.dark) .client img.dark-logo { display: block; }
	.title { grid-column: 1; font-size: 0.9em; font-weight: 500; color: var(--heading); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.state { grid-column: 2; grid-row: 1 / 3; display: grid; font: 0.7em var(--font-mono); }
	.state > span { grid-area: 1 / 1; display: inline-flex; align-items: center; justify-content: flex-end; gap: 0.5em; }
	.running { color: var(--color-busy); }
	.done { color: var(--coverage-supported); opacity: 0; }
	.dot { width: 0.6em; height: 0.6em; border-radius: 50%; background: var(--color-busy); }

	/* A 9-second loop: request, plan, Sessions finish one by one, then the report. */
	:global([data-reveal='visible']) .user { animation: show-user 9s var(--ease-out) infinite; }
	:global([data-reveal='visible']) .plan { animation: show-plan 9s var(--ease-out) infinite; }
	:global([data-reveal='visible']) .report { animation: show-report 9s var(--ease-out) infinite; }
	:global([data-reveal='visible']) .session { animation: show-session 9s var(--ease-out) infinite; animation-delay: calc(var(--s) * 0.25s); }
	:global([data-reveal='visible']) .running { animation: running 9s steps(1) infinite; animation-delay: calc(var(--s) * 0.9s); }
	:global([data-reveal='visible']) .done { animation: done 9s steps(1) infinite; animation-delay: calc(var(--s) * 0.9s); }
	@keyframes show-user { 0% { opacity: 0; translate: 0 0.6em; } 5%, 94% { opacity: 1; translate: 0 0; } 100% { opacity: 0; } }
	@keyframes show-plan { 0%, 9% { opacity: 0; translate: 0 0.6em; } 14%, 94% { opacity: 1; translate: 0 0; } 100% { opacity: 0; } }
	@keyframes show-report { 0%, 66% { opacity: 0; translate: 0 0.6em; } 72%, 94% { opacity: 1; translate: 0 0; } 100% { opacity: 0; } }
	@keyframes show-session { 0%, 12% { opacity: 0.25; } 17%, 94% { opacity: 1; } 100% { opacity: 0.25; } }
	@keyframes running { 0% { opacity: 0; } 15% { opacity: 1; } 42% { opacity: 0; } }
	@keyframes done { 0% { opacity: 0; } 42% { opacity: 1; } 96% { opacity: 0; } }
	@keyframes bob { 0%, 100% { translate: 0 0; } 50% { translate: 0 -0.35em; } }

	/* Without motion, the scene shows the finished state. */
	@media (prefers-reduced-motion: reduce) {
		.bot svg, :global([data-reveal='visible']) :is(.user, .plan, .report, .session, .done) { animation: none; }
		.packet, .running { display: none; }
		.done { opacity: 1; }
	}

	/* Narrow screens: the loop reads top to bottom, request first and report last. */
	@media (max-width: 700px) {
		.section-heading { margin-bottom: 32px; }
		.stage { display: flex; flex-direction: column; align-items: stretch; gap: 14px; aspect-ratio: auto; padding: 22px 18px; }
		.stage > * { position: static; font-size: 13px; translate: none; }
		.you { display: contents; }
		.wires { display: none; }
		.bubble { font-size: 13px; }
		.user { order: 0; }
		.plan { order: 1; align-self: flex-start; }
		.bot { position: relative; inset: auto; order: 2; align-self: center; width: 56px; height: 56px; margin-block: 16px; }
		.halo { inset: -10px; box-shadow: 0 0 0 8px color-mix(in srgb, var(--primary) 7%, transparent); }
		.sessions { order: 3; width: auto; gap: 8px; padding: 0; }
		.session { font-size: 13px; }
		.report { order: 4; align-self: flex-start; margin-top: 8px; }
	}
</style>
