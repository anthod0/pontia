<script lang="ts">
	import ArrowRightIcon from 'phosphor-svelte/lib/ArrowRightIcon';
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import LockSimpleIcon from 'phosphor-svelte/lib/LockSimpleIcon';
	import { reveal } from '$lib/reveal';
	import { m } from '$lib/paraglide/messages';
	import { localizeHref } from '$lib/paraglide/runtime';

	// SVG logo sources: Claude symbol extracted from the inline wordmark at
	// https://claude.com/product/claude-code; https://grok.com/images/favicon.svg.
	// The OpenAI symbol used for Codex comes from anomalyco/opencode:
	// packages/ui/src/assets/icons/provider/openai.svg. The Google Antigravity
	// and Kiro marks match the Dashboard client icons.
	// Local vector assets live in static/client-logos/.
	const clients = [
		{ name: 'Codex', logo: '/client-logos/codex.svg', darkLogo: '/client-logos/codex-dark.svg' },
		{ name: 'Claude Code', logo: '/client-logos/claude-code.svg' },
		{ name: 'Grok Build', logo: '/client-logos/grok-build.svg' },
		{ name: 'Google Antigravity', logo: '/client-logos/google-antigravity.svg' },
		{ name: 'Kiro', logo: '/client-logos/kiro.svg' }
	];
	// Stage coordinates (viewBox 0 0 1080 420): the channel runs from the terminal's right edge
	// to the phone's left edge at the terminal's vertical centre, and peaks under the lock.
	const channel = 'M475 183 C 565 50, 720 50, 810 183';
</script>

<section id="remote" class="page-width section-block" aria-labelledby="remote-title" {@attach reveal()}>
	<div class="intro reveal">
		<h2 id="remote-title">{m.remote_title()}</h2>
		<p>{m.remote_text()}</p>
		<a class="intro-link" href={localizeHref('/docs/dashboard')}>{m.dashboard_guide()}<ArrowRightIcon size={12} /></a>
	</div>

	<!-- One scene: the same Session in a terminal on the device and on a phone, joined by an encrypted channel. -->
	<div class="stage">
		<a class="group group-clients reveal" style="--i: 1" href={localizeHref('/docs/agent-clients')}>
			<span class="clients" aria-hidden="true">
				{#each clients as client}
					<span class="client" title={client.name}>
						<img class:light-logo={client.darkLogo} src={client.logo} alt="" width="18" height="18" loading="lazy" />
						{#if client.darkLogo}<img class="dark-logo" src={client.darkLogo} alt="" width="18" height="18" loading="lazy" />{/if}
					</span>
				{/each}
			</span>
			<span class="rule-h"></span>
			<span class="note"><span class="note-index">01</span>{m.remote_subscriptions_title()}<ArrowUpRightIcon size={12} /></span>
		</a>

		<div class="group group-terminal">
			<div class="terminal reveal" style="--i: 1" aria-hidden="true">
				<div class="terminal-bar"><span class="lights"><i></i><i></i><i></i></span>~/code/atlas — codex</div>
				<div class="terminal-body">
					<span class="t-prompt">fix the login test</span>
					<span class="t-step">Read 3 files</span>
					<span class="t-step">Edited src/auth/login.ts</span>
					<span class="t-ask">Allow <b>bun test</b>? <span>[y/n]</span><span class="cursor"></span></span>
					<span class="t-status">gpt-5.4 · feat/login-fix</span>
				</div>
			</div>
			<a class="note note-below reveal" style="--i: 3" href={localizeHref('/docs/how-it-works')}><span class="note-index">02</span>{m.remote_terminal_title()}<ArrowUpRightIcon size={12} /></a>
		</div>

		<svg class="channel reveal" style="--i: 2" viewBox="0 0 1080 420" aria-hidden="true">
			<path d={channel} />
			<circle class="anchor" cx="475" cy="183" r="4" />
			<circle class="anchor" cx="810" cy="183" r="4" />
			<circle class="packet" r="3.5"><animateMotion dur="2.6s" repeatCount="indefinite" path={channel} keyPoints="0;1" keyTimes="0;1" calcMode="spline" keySplines="0.6 0 0.4 1" /></circle>
			<circle class="packet back" r="3.5"><animateMotion dur="2.6s" begin="-1.3s" repeatCount="indefinite" path={channel} keyPoints="1;0" keyTimes="0;1" calcMode="spline" keySplines="0.6 0 0.4 1" /></circle>
		</svg>
		<span class="lock reveal" style="--i: 2" aria-hidden="true"><LockSimpleIcon size={14} weight="bold" /></span>
		<a class="note note-encrypted reveal" style="--i: 4" href={localizeHref('/docs/security')}><span class="note-index">03</span>{m.remote_encrypted_title()}<ArrowUpRightIcon size={12} /></a>

		<div class="phone reveal" style="--i: 3" aria-hidden="true">
			<span class="notch"></span>
			<div class="phone-head"><span class="dot"></span>Fix the login test</div>
			<div class="phone-body">
				<span class="bubble">fix the login test</span>
				<span class="p-step">Read 3 files</span>
				<span class="p-step">Edited src/auth/login.ts</span>
				<span class="p-reply">The session cookie expired before the redirect. I fixed the expiry check and want to run the tests.</span>
				<div class="approval">
					<span class="approval-cmd">$ bun test</span>
					<span class="approval-actions"><span class="deny">Deny</span><span class="approve">Approve</span></span>
				</div>
			</div>
			<div class="composer"></div>
		</div>
	</div>
</section>

<style>
	.intro { max-width: 560px; }
	.intro p { margin-top: 14px; font-size: 15px; line-height: 1.75; color: var(--muted-foreground); text-wrap: pretty; }
	.intro-link { display: inline-flex; align-items: center; gap: 4px; margin-top: 16px; color: var(--primary); font-size: 13px; font-weight: 500; }
	.intro-link:hover { text-decoration: underline; text-underline-offset: 3px; }

	/* The stage keeps one aspect ratio and sizes everything in em, so the composition scales as a whole. */
	.stage { position: relative; container-type: inline-size; aspect-ratio: 1080 / 420; margin-top: 48px; }
	.stage > * { position: absolute; font-size: min(16px, 1.48cqw); }

	.group-clients { top: 4%; left: 0; display: flex; align-items: center; gap: 0.9em; }
	.clients { display: flex; gap: 0.35em; }
	.client { display: grid; place-items: center; width: 2.3em; height: 2.3em; border: 1px solid var(--border); border-radius: 50%; background: var(--surface); transition: translate 0.25s var(--ease-out), border-color 0.2s; }
	.client img { width: 1.1em; height: 1.1em; object-fit: contain; }
	.group-clients:hover .client { border-color: var(--border-strong); translate: 0 -0.15em; }
	.group-clients:hover .client:nth-child(2) { transition-delay: 30ms; }
	.group-clients:hover .client:nth-child(3) { transition-delay: 60ms; }
	.group-clients:hover .client:nth-child(4) { transition-delay: 90ms; }
	.group-clients:hover .client:nth-child(5) { transition-delay: 120ms; }
	.client img.dark-logo { display: none; }
	:global(.dark) .client img.light-logo { display: none; }
	:global(.dark) .client img.dark-logo { display: block; }
	.rule-h { width: 2.6em; height: 1px; background: var(--border-stronger); }

	.group-terminal { top: 20%; left: 0; display: flex; flex-direction: column; align-items: flex-start; width: 44%; }

	.terminal { width: 100%; overflow: hidden; border: 1px solid #45433e; border-radius: 0.6em; background: #282828; box-shadow: 0 2em 3em -2em rgb(40 40 40 / 55%); font: 0.75em/1.65 var(--font-mono); color: #ebdbb2; }
	.terminal-bar { display: flex; align-items: center; gap: 0.9em; padding: 0.7em 1em; border-bottom: 1px solid #3c3836; color: #928374; }
	.lights { display: flex; gap: 0.4em; }
	.lights i { width: 0.65em; height: 0.65em; border-radius: 50%; background: #504945; }
	.terminal-body { display: flex; flex-direction: column; gap: 0.35em; padding: 1.1em 1.3em 1em; }
	.terminal-body > span::before { display: inline-block; width: 1.4em; color: #928374; }
	.t-prompt::before { content: '›'; }
	.t-step { color: #a89984; }
	.t-step::before { content: '•'; }
	.t-ask { color: #fabd2f; }
	.t-ask::before { content: '?'; color: #fabd2f !important; }
	.t-ask b { font-weight: 500; color: #ebdbb2; }
	.t-ask span { color: #928374; }
	.t-status { margin-top: 0.8em; padding-top: 0.7em; border-top: 1px dashed #3c3836; color: #83a598; }
	.t-status::before { content: '' !important; width: 0 !important; }
	.cursor { display: inline-block; width: 0.5em; height: 1em; margin-left: 0.4em; vertical-align: -0.15em; background: #ebdbb2; animation: blink 1s steps(1) infinite; }

	.channel { z-index: 1; inset: 0; width: 100%; height: 100%; overflow: visible; pointer-events: none; }
	.channel path { fill: none; stroke: var(--primary); stroke-width: 1.2; stroke-dasharray: 4 5; vector-effect: non-scaling-stroke; animation: flow 1.2s linear infinite; }
	.anchor { fill: var(--background); stroke: var(--primary); stroke-width: 1.2; vector-effect: non-scaling-stroke; }
	.packet { fill: var(--primary); }
	.packet.back { fill: var(--olive); }
	.lock { z-index: 2; top: 18.3%; left: 59.491%; display: grid; place-items: center; width: 2.1em; height: 2.1em; translate: -50% -50%; border: 1px solid var(--primary); border-radius: 50%; background: var(--background); color: var(--primary); box-shadow: 0 0 0 0.4em color-mix(in srgb, var(--primary) 10%, transparent); }

	.phone { top: 0; right: 0; display: flex; flex-direction: column; width: 25%; padding: 1.4em 0.9em 0; overflow: hidden; border: 1px solid var(--border-strong); border-bottom: 0; border-radius: 2.2em 2.2em 0 0; background: var(--surface); box-shadow: 0 -1em 3em -2em rgb(60 56 54 / 35%); font-size: min(16px, 1.48cqw); mask-image: linear-gradient(#000 70%, transparent); }
	.notch { align-self: center; width: 4.5em; height: 0.4em; margin-bottom: 1.1em; border-radius: 1em; background: var(--border-strong); }
	.phone-head { display: flex; align-items: center; gap: 0.5em; padding-bottom: 0.8em; border-bottom: 1px solid var(--border); font-size: 0.8em; font-weight: 600; color: var(--heading); }
	.dot { width: 0.5em; height: 0.5em; border-radius: 50%; background: var(--color-interrupted); }
	.phone-body { display: flex; flex-direction: column; gap: 0.7em; padding-top: 1em; font-size: 0.72em; }
	.bubble { align-self: flex-end; padding: 0.5em 0.9em; border-radius: 1em 1em 0.3em 1em; background: var(--muted); color: var(--foreground); }
	.p-step { color: var(--muted-foreground); }
	.p-reply { line-height: 1.55; color: var(--foreground); }
	.p-step::before { content: '✓'; margin-right: 0.6em; color: var(--olive); }
	.approval { display: flex; flex-direction: column; gap: 0.8em; padding: 0.9em; border: 1px solid rgb(177 98 134 / 40%); border-radius: 0.7em; background: var(--background); animation: attention 2.8s ease-in-out infinite; }
	.approval-cmd { font-family: var(--font-mono); color: var(--heading); }
	.approval-actions { display: flex; justify-content: flex-end; gap: 0.5em; }
	.approval-actions span { padding: 0.3em 0.8em; border-radius: 0.4em; font-weight: 500; }
	.deny { border: 1px solid var(--border-strong); color: var(--foreground); }
	.approve { background: var(--button); color: white; }
	.composer { flex-shrink: 0; height: 2.4em; margin-top: 1.2em; margin-bottom: 1.6em; border: 1px solid var(--border); border-radius: 1.2em; background: var(--background); }

	/* Annotations: a number, a short title, and a hairline leading to what they describe. */
	.note { display: inline-flex; align-items: center; gap: 0.6em; font-size: 0.875em; font-weight: 500; color: var(--heading); white-space: nowrap; transition: color 0.15s; }
	.note-index { font: 0.75em var(--font-mono); color: var(--primary); }
	.note :global(svg) { color: var(--primary); opacity: 0; translate: -0.2em 0.2em; transition: opacity 0.15s, translate 0.15s; }
	.note:hover { color: var(--primary); }
	.note:hover :global(svg) { opacity: 1; translate: 0 0; }
	.group-clients .note { font-size: inherit; }
	.group-clients:hover .note { color: var(--primary); }
	.group-clients:hover .note :global(svg) { opacity: 1; translate: 0 0; }
	/* A vertical hairline joins a note to the part above or below it. */
	.note-below { position: relative; margin: 3.2em 0 0 1.6em; }
	.note-below::before { content: ''; position: absolute; bottom: calc(100% + 0.55em); left: 0.35em; width: 1px; height: 2.4em; background: var(--border-stronger); }
	.note-encrypted { top: 0; left: 59.491%; translate: -50% 0; }
	.note-encrypted::after { content: ''; position: absolute; top: calc(100% + 0.5em); left: 50%; width: 1px; height: 1.6em; background: var(--border-stronger); }

	@keyframes blink { 50% { opacity: 0; } }
	@keyframes flow { to { stroke-dashoffset: -18; } }
	@keyframes attention { 0%, 100% { box-shadow: 0 0 0 0 rgb(177 98 134 / 0%); } 50% { box-shadow: 0 0 0 0.25em rgb(177 98 134 / 14%); } }

	@media (prefers-reduced-motion: reduce) {
		.cursor, .channel path, .approval { animation: none; }
		.packet { display: none; }
	}

	/* Narrow screens: the scene becomes a column, with each note above the part it describes. */
	@media (max-width: 700px) {
		.stage { display: flex; flex-direction: column; align-items: flex-start; gap: 28px; aspect-ratio: auto; margin-top: 36px; }
		.stage > * { position: static; font-size: 13px; }
		.channel, .lock, .rule-h, .note-encrypted::after { display: none; }
		.group-clients { flex-direction: column-reverse; align-items: flex-start; gap: 12px; }
		.group-terminal { width: 100%; flex-direction: column-reverse; gap: 12px; }
		.note-below { margin: 0; }
		.note-below::before { display: none; }
		.note-encrypted { translate: none; }
		.phone { align-self: center; width: 250px; }
	}
</style>
