<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import LockSimpleIcon from 'phosphor-svelte/lib/LockSimpleIcon';
	import { reveal } from '$lib/reveal';

	const clients = [
		{ name: 'pi', logo: '/client-logos/pi.svg' },
		{ name: 'Codex', logo: '/client-logos/codex.svg' },
		{ name: 'Claude Code', logo: '/client-logos/claude-code.svg' },
		{ name: 'Grok Build', logo: '/client-logos/grok-build.svg' },
		{ name: 'OpenCode', logo: '/client-logos/opencode.svg' }
	];
	const terminalLines = ['Reading 4 files', 'Editing src/shortcuts.ts', 'Running bun test'];
</script>

<section id="features" class="highlights page-width section-block" aria-labelledby="features-title" {@attach reveal()}>
	<div class="section-intro reveal">
		<div class="section-heading">
			<h2 id="features-title">Control your agents from anywhere.</h2>
			<p>Start work on your device. Follow it and step in from any browser.</p>
		</div>
		<a class="section-link" href="/docs/how-it-works">How Pontia works<ArrowRightIcon size={12} /></a>
	</div>

	<div class="bento">
		<article class="card wide reveal" style="--i: 1">
			<div class="visual clients-visual" aria-hidden="true">
				<div class="marquee">
					{#each [0, 1] as copy}
						<ul class:duplicate={copy === 1}>
							{#each clients as client}<li><img src={client.logo} alt="" width="24" height="24" loading="lazy" />{client.name}</li>{/each}
						</ul>
					{/each}
				</div>
			</div>
			<div class="card-copy">
				<h3>Bring your own agents.</h3>
				<p>Keep your accounts, model subscriptions, and configuration. Pontia doesn't host models and has no usage fee.</p>
				<a class="card-link" href="/docs/agent-clients">Agent Clients<ArrowRightIcon size={12} /></a>
			</div>
		</article>

		<article class="card narrow reveal" style="--i: 2">
			<div class="visual phone-visual" aria-hidden="true">
				<div class="phone">
					<span class="phone-notch"></span>
					<span class="phone-title">Sessions</span>
					<span class="phone-row"><span class="dot busy"></span>Fix search pagination</span>
					<span class="phone-row approval"><span class="dot waiting"></span>Review API changes<span class="approve">Approve</span></span>
					<span class="phone-row"><span class="dot idle"></span>Add integration tests</span>
				</div>
			</div>
			<div class="card-copy">
				<h3>In your pocket.</h3>
				<p>Add the Dashboard to your home screen. Approve, follow up, or interrupt a Turn from anywhere.</p>
				<a class="card-link" href="/docs/dashboard">Dashboard guide<ArrowRightIcon size={12} /></a>
			</div>
		</article>

		<article class="card narrow reveal" style="--i: 1">
			<div class="visual terminal-visual" aria-hidden="true">
				<div class="terminal">
					<span class="terminal-path">~/code/atlas <span>feat/keyboard-shortcuts</span></span>
					<span class="terminal-line prompt">codex</span>
					{#each terminalLines as line, index}<span class="terminal-line step" style={`--step: ${index}`}>{line}</span>{/each}
					<span class="terminal-line done" style="--step: 3">128 tests passed<span class="cursor"></span></span>
				</div>
			</div>
			<div class="card-copy">
				<h3>Execution stays local.</h3>
				<p>Agents run with your tools, files, and credentials. Pontia gives you access, not a remote sandbox.</p>
			</div>
		</article>

		<article class="card wide reveal" style="--i: 2">
			<div class="visual relay-visual" aria-hidden="true">
				<div class="relay">
					<div class="node"><span class="node-name">Browser</span></div>
					<div class="wire to-edge"><span class="packet"></span><span class="packet back"></span></div>
					<div class="node edge">
						<span class="node-name"><LockSimpleIcon size={12} weight="bold" />Edge</span>
						<span class="cipher"><span>9f2a c41e 77b0 e3d9 1a6c 58f2 0bd4 9f2a c41e 77b0 e3d9 1a6c 58f2 0bd4</span></span>
					</div>
					<div class="wire from-edge"><span class="packet"></span><span class="packet back"></span></div>
					<div class="node"><span class="node-name">Device</span></div>
				</div>
			</div>
			<div class="card-copy">
				<h3>Encrypted end to end.</h3>
				<p>Only your browser and your device can read the session. Use an official Edge or run your own. Either way, the Edge only relays ciphertext.</p>
				<a class="card-link" href="/docs/security">Security model<ArrowRightIcon size={12} /></a>
			</div>
		</article>
	</div>
</section>

<style>
	.section-link { display: inline-flex; flex-shrink: 0; align-items: center; gap: 4px; padding-bottom: 6px; color: var(--primary); font-size: 13px; font-weight: 500; }
	.section-link:hover, .card-link:hover { text-decoration: underline; text-underline-offset: 3px; }

	.bento { display: grid; grid-template-columns: repeat(6, minmax(0, 1fr)); gap: 16px; }
	.card { display: flex; flex-direction: column; overflow: hidden; border: 1px solid var(--border); border-radius: 10px; background: var(--surface); transition: border-color 0.25s, box-shadow 0.25s, translate 0.25s; }
	/* Keep the reveal transition alongside the hover transitions. */
	.bento :global(.card.reveal) { transition: opacity 0.7s var(--ease-out), transform 0.7s var(--ease-out), border-color 0.25s, box-shadow 0.25s, translate 0.25s; transition-delay: calc(var(--i, 0) * 90ms), calc(var(--i, 0) * 90ms), 0s, 0s, 0s; }
	.card:hover { border-color: #d6d3cc; box-shadow: 0 18px 40px -24px rgb(60 56 54 / 28%); translate: 0 -2px; }
	.wide { grid-column: span 4; }
	.narrow { grid-column: span 2; }
	.visual { position: relative; display: grid; place-items: center; height: 200px; overflow: hidden; border-bottom: 1px solid var(--border); background: radial-gradient(circle at 1px 1px, #e6e4df 1px, transparent 0) 0 0 / 16px 16px, var(--background); }
	.card-copy { display: flex; flex: 1; flex-direction: column; padding: 22px 24px 24px; }
	.card h3 { margin-bottom: 8px; font-size: 19px; font-weight: 500; letter-spacing: -0.025em; }
	.card p { flex: 1; font-size: 14px; line-height: 1.7; color: var(--muted-foreground); text-wrap: pretty; }
	.card-link { display: inline-flex; align-self: flex-start; align-items: center; gap: 4px; margin-top: 16px; color: var(--primary); font-size: 12px; font-weight: 500; }

	/* Relay: packets travel both ways; the Edge only ever shows scrolling ciphertext. */
	.relay { display: flex; align-items: center; width: min(560px, 100% - 48px); }
	.node { display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 10px 14px; border: 1px solid var(--border); border-radius: 6px; background: var(--surface); box-shadow: 0 6px 16px -10px rgb(60 56 54 / 30%); }
	.node-name { display: inline-flex; align-items: center; gap: 5px; font-size: 12px; font-weight: 500; color: var(--heading); }
	.edge { border-color: var(--primary); border-style: dashed; }
	.edge .node-name { color: var(--primary); }
	.cipher { width: 96px; overflow: hidden; mask-image: linear-gradient(90deg, transparent, #000 20%, #000 80%, transparent); }
	.cipher span { display: block; width: max-content; font: 9px var(--font-mono); color: #79740e; animation: scroll-x 6s linear infinite; }
	.wire { position: relative; flex: 1; height: 1px; margin-inline: 4px; background: repeating-linear-gradient(90deg, var(--primary) 0 4px, transparent 4px 8px); }
	/* Each packet crosses one wire per half cycle, passing through the Edge between them. */
	.packet { position: absolute; top: -3px; left: 0; width: 7px; height: 7px; border-radius: 50%; background: var(--primary); box-shadow: 0 0 0 3px rgb(69 133 136 / 18%); opacity: 0; animation: 4.8s cubic-bezier(0.6, 0, 0.4, 1) infinite; }
	.packet.back { background: #79740e; box-shadow: 0 0 0 3px rgb(121 116 14 / 18%); animation-direction: reverse; }
	.to-edge .packet, .to-edge .packet.back { animation-name: travel-first-half; }
	.from-edge .packet, .from-edge .packet.back { animation-name: travel-second-half; }

	/* Terminal: lines appear in sequence once the card is revealed. */
	.terminal-visual { place-items: center; padding: 20px; }
	.terminal { display: flex; flex-direction: column; gap: 5px; width: 100%; max-width: 280px; padding: 14px 16px; border: 1px solid #45433e; border-radius: 8px; background: #282828; box-shadow: 0 12px 28px -16px rgb(40 40 40 / 60%); font: 11px/1.5 var(--font-mono); color: #ebdbb2; }
	.terminal-path { margin-bottom: 2px; color: #83a598; }
	.terminal-path span { color: #928374; }
	.terminal-line::before { margin-right: 8px; color: #928374; }
	.prompt::before { content: '$'; }
	.step::before { content: '›'; color: #d79921; }
	.done { color: #b8bb26; }
	.done::before { content: '✓'; color: #b8bb26; }
	.cursor { display: inline-block; width: 6px; height: 12px; margin-left: 6px; vertical-align: -2px; background: #ebdbb2; animation: blink 1s steps(1) infinite; }

	/* Phone: the busy dot pulses and the approval row asks for attention. */
	.phone-visual { place-items: end center; }
	.phone { display: flex; flex-direction: column; gap: 7px; width: 196px; height: 172px; padding: 22px 12px 0; border: 1px solid #d6d3cc; border-bottom: 0; border-radius: 24px 24px 0 0; background: var(--surface); box-shadow: 0 -8px 30px -18px rgb(60 56 54 / 30%); }
	.phone-notch { align-self: center; width: 44px; height: 5px; margin: -10px 0 6px; border-radius: 3px; background: #d6d3cc; }
	.phone-title { font-size: 12px; font-weight: 600; color: var(--heading); }
	.phone-row { display: flex; align-items: center; gap: 7px; padding: 7px 8px; overflow: hidden; border: 1px solid transparent; border-radius: 6px; background: var(--background); font-size: 10px; color: var(--foreground); white-space: nowrap; }
	.approval { border-color: rgb(177 98 134 / 35%); animation: attention 2.8s ease-in-out infinite; }
	.approve { margin-left: auto; padding: 2px 6px; border-radius: 4px; background: #407e81; color: white; font-size: 9px; font-weight: 500; }
	.dot { flex-shrink: 0; width: 6px; height: 6px; border-radius: 50%; }
	.dot.busy { background: var(--color-busy); animation: pulse 1.6s ease-out infinite; }
	.dot.waiting { background: var(--color-interrupted); }
	.dot.idle { background: var(--color-idle); }

	/* Client logos scroll continuously; the second copy fills the loop. */
	.marquee { display: flex; width: 100%; overflow: hidden; mask-image: linear-gradient(90deg, transparent, #000 15%, #000 85%, transparent); }
	.marquee ul { display: flex; flex-shrink: 0; gap: 12px; margin: 0; padding: 0 6px; list-style: none; animation: scroll-list 24s linear infinite; }
	.card:hover .marquee ul { animation-play-state: paused; }
	.marquee li { display: flex; align-items: center; gap: 10px; padding: 12px 18px 12px 14px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); font-size: 14px; font-weight: 500; color: var(--heading); white-space: nowrap; box-shadow: 0 6px 16px -12px rgb(60 56 54 / 30%); }
	.marquee img { width: 24px; height: 24px; object-fit: contain; }


	@keyframes travel-first-half {
		0% { left: 0; opacity: 0; }
		4%, 46% { opacity: 1; }
		50%, 100% { left: calc(100% - 7px); opacity: 0; }
	}
	@keyframes travel-second-half {
		0%, 50% { left: 0; opacity: 0; }
		54%, 96% { opacity: 1; }
		100% { left: calc(100% - 7px); opacity: 0; }
	}
	@keyframes scroll-x { to { transform: translateX(-50%); } }
	@keyframes scroll-list { to { transform: translateX(-100%); } }
	@keyframes blink { 50% { opacity: 0; } }
	@keyframes pulse { 0% { box-shadow: 0 0 0 0 rgb(215 153 33 / 50%); } 100% { box-shadow: 0 0 0 7px rgb(215 153 33 / 0%); } }
	@keyframes attention { 0%, 100% { box-shadow: 0 0 0 0 rgb(177 98 134 / 0%); } 50% { box-shadow: 0 0 0 3px rgb(177 98 134 / 14%); } }

	/* Terminal steps type in after their card is revealed. */
	:global([data-reveal]) .step, :global([data-reveal]) .done { opacity: 0; }
	:global([data-reveal='visible']) .step, :global([data-reveal='visible']) .done { animation: type-in 0.4s ease-out both; animation-delay: calc(700ms + var(--step) * 450ms); }
	@keyframes type-in { from { opacity: 0; transform: translateX(-6px); } to { opacity: 1; transform: none; } }

	@media (prefers-reduced-motion: reduce) {
		.packet, .cipher span, .marquee ul, .cursor, .dot.busy, .approval { animation: none; }
		.packet { opacity: 1; }
		.packet.back { display: none; }
		.marquee .duplicate { display: none; }
		.marquee { justify-content: center; }
		.marquee ul { flex-wrap: wrap; justify-content: center; }
	}
	@media (max-width: 900px) {
		.wide, .narrow { grid-column: span 6; }
	}
	@media (max-width: 600px) {
		.visual { height: 176px; }
		.card-copy { padding: 18px 18px 20px; }
		.card h3 { font-size: 18px; }
		.relay { width: calc(100% - 32px); }
		.node { padding: 8px 10px; }
		.cipher { width: 48px; }
	}
</style>
