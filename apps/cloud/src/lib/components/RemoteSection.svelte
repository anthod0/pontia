<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon';
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import EncryptedChannel from './remote/EncryptedChannel.svelte';
	import SpinnerGapIcon from 'phosphor-svelte/lib/SpinnerGapIcon';
	import { reveal } from '$lib/reveal';
	import { approve, deny, finish, send, start, type RemoteDemo } from '$lib/remote-demo';
	import { command, prompts, replies, stepText } from './remote/text';
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
	const WORK_MS = 1400;

	type Surface = 'terminal' | 'phone';

	// The terminal and the phone render the same Session, so an answer on either one shows on both.
	let demo = $state<RemoteDemo>(start());
	// Each change crosses the encrypted channel once, from the side that made it.
	let pulse = $state<{ id: number; from: Surface } | null>(null);
	let stage = $state<HTMLElement | null>(null);
	let terminalFeed = $state<HTMLElement | null>(null);
	let phoneFeed = $state<HTMLElement | null>(null);
	let timer: ReturnType<typeof setTimeout> | undefined;
	// The surface to keep keyboard focus on once the agent needs the visitor again.
	let focusSurface: Surface | null = null;

	const waiting = $derived(demo.waiting);
	const latestReply = $derived(demo.entries.findLast((entry) => entry.kind === 'reply'));
	const stateLabel = $derived(
		{
			approval: m.remote_demo_state_approval,
			message: m.remote_demo_state_message,
			agent: m.remote_demo_state_agent,
			finished: m.remote_demo_state_finished
		}[waiting.kind]()
	);

	// New lines arrive at the bottom of both feeds; keep them in view without moving the page.
	$effect(() => {
		void demo.entries.length;
		for (const feed of [terminalFeed, phoneFeed]) feed?.scrollTo({ top: feed.scrollHeight, behavior: 'smooth' });
	});

	async function update(next: RemoteDemo, from: Surface) {
		clearTimeout(timer);
		focusSurface = stage?.contains(document.activeElement) ? from : null;
		demo = next;
		pulse = { id: (pulse?.id ?? 0) + 1, from };
		if (next.waiting.kind === 'agent') {
			timer = setTimeout(() => {
				demo = finish(demo);
				pulse = { id: (pulse?.id ?? 0) + 1, from: 'terminal' };
				void restoreFocus();
			}, WORK_MS);
		} else {
			await restoreFocus();
		}
	}

	async function restoreFocus() {
		if (!focusSurface) return;
		await tick();
		stage?.querySelector<HTMLElement>(`[data-surface='${focusSurface}'] [data-primary]`)?.focus({ preventScroll: true });
	}

	function handleTerminalKey(event: KeyboardEvent) {
		if (waiting.kind !== 'approval' || event.altKey || event.ctrlKey || event.metaKey) return;
		const key = event.key.toLowerCase();
		if (key === 'y') void update(approve(demo), 'terminal');
		if (key === 'n') void update(deny(demo), 'terminal');
	}

	function replay(from: Surface) {
		void update(start(), from);
	}

	onDestroy(() => clearTimeout(timer));
</script>

<section id="remote" class="page-width section-block" aria-labelledby="remote-title" {@attach reveal()}>
	<div class="intro reveal">
		<h2 id="remote-title">{m.remote_title()}</h2>
		<p>{m.remote_text()}</p>
	</div>

	<!-- One scene: the same Session in a terminal on the device and on a phone, joined by an encrypted channel. -->
	<div class="stage" bind:this={stage} role="group" aria-label={m.remote_demo_label()}>
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
			<!-- svelte-ignore a11y_no_noninteractive_element_interactions (The group handles approval shortcuts while it or its buttons have focus.) -->
			<div class={['terminal reveal', pulse?.from === 'terminal' && 'sending']} style="--i: 1" data-surface="terminal" role="group" tabindex="-1" onkeydown={handleTerminalKey}>
				<div class="terminal-bar" aria-hidden="true"><span class="lights"><i></i><i></i><i></i></span>~/code/atlas — codex</div>
				<div class="terminal-body" bind:this={terminalFeed}>
					{#each demo.entries as entry, index (index)}
						{#if entry.kind === 'prompt'}
							<span class="t-prompt enter">{prompts[entry.id]()}</span>
						{:else if entry.kind === 'step'}
							<span class={['t-step enter', !entry.done && 'running']}>{stepText(entry)}</span>
						{:else if entry.kind === 'reply'}
							<span class="t-reply enter">{replies[entry.id]()}</span>
						{:else if entry.answer}
							<span class={['t-ask', `answer-${entry.answer}`]}>{m.remote_demo_ask({ command })} <span>{entry.answer === 'approve' ? 'y' : 'n'}</span></span>
						{:else}
							<span class="t-ask">
								{m.remote_demo_ask({ command })}
								<span class="keys">[<button type="button" data-primary aria-label={m.remote_demo_approve()} onclick={() => update(approve(demo), 'terminal')}>y</button>/<button type="button" aria-label={m.remote_demo_deny()} onclick={() => update(deny(demo), 'terminal')}>n</button>]</span><span class="cursor"></span>
							</span>
						{/if}
					{/each}
					{#if waiting.kind === 'message'}
						{@const suggestion = waiting.suggestion}
						<button type="button" class="t-input enter" data-primary onclick={() => update(send(demo, suggestion), 'terminal')}>
							<span class="t-ghost">{prompts[suggestion]()}</span><kbd>⏎</kbd>
						</button>
					{:else if waiting.kind === 'finished'}
						<button type="button" class="t-input enter" data-primary aria-label={m.remote_demo_restart()} onclick={() => replay('terminal')}>
							<span class="cursor"></span>
						</button>
					{/if}
				</div>
				<div class="t-status" aria-hidden="true">gpt-5.4 · feat/login-fix</div>
			</div>
			<a class="note note-below reveal" style="--i: 3" href={localizeHref('/docs/how-it-works')}><span class="note-index">02</span>{m.remote_terminal_title()}<ArrowUpRightIcon size={12} /></a>
		</div>

		<a class="note note-encrypted reveal" style="--i: 4" href={localizeHref('/docs/security')}><span class="note-index">03</span>{m.remote_encrypted_title()}<ArrowUpRightIcon size={12} /></a>

		<div class="connection reveal" style="--i: 2"><EncryptedChannel {pulse} /></div>

		<div class="phone reveal" style="--i: 3" data-surface="phone">
			<span class="notch" aria-hidden="true"></span>
			<div class="phone-head">
				<span class={['dot', `dot-${waiting.kind}`]}></span>
				<span class="phone-title">{m.remote_demo_title()}</span>
				<span class="phone-state">{stateLabel}</span>
			</div>
			<div class="phone-body" bind:this={phoneFeed}>
				{#each demo.entries as entry, index (index)}
					{#if entry.kind === 'prompt'}
						<span class="bubble enter">{prompts[entry.id]()}</span>
					{:else if entry.kind === 'step'}
						<span class={['p-step enter', !entry.done && 'running']}>
							{#if entry.done}<CheckIcon size={11} weight="bold" />{:else}<SpinnerGapIcon size={11} class="spin" />{/if}{stepText(entry)}
						</span>
					{:else if entry.kind === 'reply'}
						<span class="p-reply enter">{replies[entry.id]()}</span>
					{:else if entry.answer}
						<span class={['p-answer enter', `answer-${entry.answer}`]}>
							<span class="approval-cmd">$ {command}</span>{entry.answer === 'approve' ? m.remote_demo_approved() : m.remote_demo_denied()}
						</span>
					{:else}
						<div class="approval">
							<span class="approval-cmd">$ {command}</span>
							<span class="approval-actions">
								<button type="button" class="deny" onclick={() => update(deny(demo), 'phone')}>{m.remote_demo_deny()}</button>
								<button type="button" class="approve" data-primary onclick={() => update(approve(demo), 'phone')}>{m.remote_demo_approve()}</button>
							</span>
						</div>
					{/if}
				{/each}
			</div>
			{#if waiting.kind === 'finished'}
				<button type="button" class="composer" data-primary aria-label={m.remote_demo_restart()} onclick={() => replay('phone')}>
					<span class="placeholder">{m.remote_demo_composer()}</span>
				</button>
			{:else}
				<div class="composer">
					{#if waiting.kind === 'message'}
						{@const suggestion = waiting.suggestion}
						<button type="button" class="suggestion" data-primary onclick={() => update(send(demo, suggestion), 'phone')}>
							<span>{prompts[suggestion]()}</span><span class="send" aria-label={m.remote_demo_send()}><ArrowUpIcon size={11} weight="bold" /></span>
						</button>
					{:else}
						<span class="placeholder">{m.remote_demo_composer()}</span>
					{/if}
				</div>
			{/if}
		</div>
	</div>
	<p class="sr-only" aria-live="polite">{stateLabel}. {latestReply ? replies[latestReply.id]() : ''}</p>
</section>

<style>
	.intro { max-width: 560px; }
	.intro p { margin-top: 14px; font-size: 15px; line-height: 1.75; color: var(--muted-foreground); text-wrap: pretty; }

	/* The stage keeps one aspect ratio and sizes everything in em, so the composition scales as a whole. */
	.stage { position: relative; container-type: inline-size; aspect-ratio: 1080 / 420; margin-top: 48px; }
	.stage > * { position: absolute; font-size: min(16px, 1.48cqw); }
	.stage :where(button) { padding: 0; border: 0; background: none; color: inherit; }
	.stage :where(button):focus-visible { outline-offset: 2px; }

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

	.terminal { width: 100%; overflow: hidden; border: 1px solid #45433e; border-radius: 0.6em; background: #282828; box-shadow: 0 2em 3em -2em rgb(40 40 40 / 55%); font: 0.75em/1.65 var(--font-mono); color: #ebdbb2; transition: border-color 0.3s; }
	.terminal.sending { border-color: #665c54; }
	.terminal-bar { display: flex; align-items: center; gap: 0.9em; padding: 0.7em 1em; border-bottom: 1px solid #3c3836; color: #928374; }
	.lights { display: flex; gap: 0.4em; }
	.lights i { width: 0.65em; height: 0.65em; border-radius: 50%; background: #504945; }
	/* A fixed-height scrollback, so the scene keeps its layout as the Session grows. */
	.terminal-body { display: flex; flex-direction: column; gap: 0.35em; height: 13.4em; padding: 1.1em 1.3em 0.8em; overflow-y: auto; scrollbar-width: none; mask-image: linear-gradient(transparent, #000 1.1em); }
	.terminal-body::-webkit-scrollbar { display: none; }
	.terminal-body > * { flex-shrink: 0; padding-left: 1.4em; text-indent: -1.4em; text-align: left; }
	.terminal-body > *::before { display: inline-block; width: 1.4em; text-indent: 0; color: #928374; }
	.t-prompt::before { content: '›'; }
	.t-step { color: #a89984; }
	.t-step::before { content: '•'; }
	.t-step.running { color: #fabd2f; }
	.t-step.running::before { content: '◐'; color: #fabd2f; animation: spin 1s linear infinite; text-align: center; }
	.t-reply { color: #ebdbb2; }
	.t-reply::before { content: '⏺'; color: #83a598; font-size: 0.8em; }
	.t-ask { color: #fabd2f; }
	.t-ask::before { content: '?'; color: #fabd2f }
	.t-ask > span { color: #928374; }
	.t-ask.answer-approve > span { color: #b8bb26; }
	.t-ask.answer-deny > span { color: #fb4934; }
	.keys button { padding: 0 0.15em; border-radius: 0.2em; color: #ebdbb2; transition: background 0.15s, color 0.15s; }
	.keys button:hover { background: #504945; color: #fabd2f; }
	.t-input { display: flex; align-items: center; width: 100%; border-radius: 0.3em; font: inherit; text-indent: 0; }
	.t-input::before { content: '›'; margin-left: -1.4em; text-indent: 0; }
	.t-ghost { color: #7c6f64; transition: color 0.15s; }
	.t-input kbd { margin-left: 0.8em; padding: 0 0.4em; border: 1px solid #504945; border-radius: 0.25em; font: inherit; font-size: 0.85em; color: #928374; }
	.t-input:hover .t-ghost { color: #ebdbb2; }
	.t-input:hover kbd { border-color: #fabd2f; color: #fabd2f; }
	.t-status { margin: 0 1.3em; padding: 0.7em 0 1em; border-top: 1px dashed #3c3836; color: #83a598; }
	.terminal:focus-within .cursor { display: inline-block; }
	.cursor { display: none; width: 0.5em; height: 1em; margin-left: 0.4em; vertical-align: -0.15em; background: #ebdbb2; animation: blink 1s steps(1) infinite; }

	.connection { z-index: 1; left: 43.9815%; top: 0; width: 31.0185%; height: 43.5714%; pointer-events: none; }

	/* The phone fills the stage height; its feed scrolls and the composer stays in reach. */
	.phone { top: 0; right: 0; bottom: 0; display: flex; flex-direction: column; width: 25%; padding: 1.4em 0.9em 0; overflow: hidden; border: 1px solid var(--border-strong); border-bottom: 0; border-radius: 2.2em 2.2em 0 0; background: var(--surface); box-shadow: 0 -1em 3em -2em rgb(60 56 54 / 35%); font-size: min(16px, 1.48cqw); mask-image: linear-gradient(#000 88%, transparent); }
	.notch { align-self: center; flex-shrink: 0; width: 4.5em; height: 0.4em; margin-bottom: 1.1em; border-radius: 1em; background: var(--border-strong); }
	.phone-head { display: flex; flex-shrink: 0; align-items: center; gap: 0.5em; padding-bottom: 0.8em; border-bottom: 1px solid var(--border); font-size: 0.8em; font-weight: 600; color: var(--heading); }
	.phone-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.phone-state { margin-left: auto; font: 400 0.8em var(--font-mono); color: var(--muted-foreground); white-space: nowrap; }
	.dot { flex-shrink: 0; width: 0.5em; height: 0.5em; border-radius: 50%; background: var(--color-interrupted); transition: background 0.3s; }
	.dot-agent { background: var(--color-busy); animation: busy 1.6s ease-out infinite; }
	.dot-finished { background: var(--coverage-supported); }
	.phone-body { display: flex; flex: 1; flex-direction: column; gap: 0.7em; min-height: 0; padding: 1em 0 0.4em; overflow-y: auto; scrollbar-width: none; font-size: 0.72em; mask-image: linear-gradient(transparent, #000 1.2em); }
	.phone-body::-webkit-scrollbar { display: none; }
	.phone-body > * { flex-shrink: 0; }
	.bubble { align-self: flex-end; max-width: 85%; padding: 0.5em 0.9em; border-radius: 1em 1em 0.3em 1em; background: var(--muted); color: var(--foreground); }
	.p-step { display: flex; align-items: center; gap: 0.6em; color: var(--muted-foreground); }
	.p-step :global(svg) { flex-shrink: 0; color: var(--olive); }
	.p-step.running { color: var(--foreground); }
	.p-step.running :global(svg) { color: var(--color-busy); }
	.p-step :global(.spin) { animation: spin 1s linear infinite; }
	.p-reply { line-height: 1.55; color: var(--foreground); }
	.approval { display: flex; flex-direction: column; gap: 0.8em; padding: 0.9em; border: 1px solid rgb(177 98 134 / 40%); border-radius: 0.7em; background: var(--background); animation: attention 2.8s ease-in-out infinite; }
	.approval-cmd { font-family: var(--font-mono); color: var(--heading); }
	.approval-actions { display: flex; justify-content: flex-end; gap: 0.5em; }
	.approval-actions button { padding: 0.3em 0.8em; border-radius: 0.4em; font-size: inherit; font-weight: 500; transition: background 0.15s, border-color 0.15s, scale 0.15s var(--ease-out); }
	.approval-actions button:active { scale: 0.96; }
	.deny { border: 1px solid var(--border-strong); color: var(--foreground); }
	.deny:hover { border-color: var(--border-stronger); background: var(--muted); }
	.approve { background: var(--button); color: white }
	.approve:hover { background: color-mix(in srgb, var(--button) 86%, black) }
	.p-answer { display: flex; align-items: center; justify-content: space-between; gap: 0.6em; padding: 0.6em 0.9em; border: 1px solid var(--border); border-radius: 0.7em; background: var(--background); font-weight: 500; color: var(--coverage-supported); }
	.p-answer.answer-deny { color: var(--muted-foreground); }
	.composer { display: flex; flex-shrink: 0; align-items: center; height: 2.4em; margin: 0.8em 0 1.6em; padding: 0 0.3em 0 0.9em; border: 1px solid var(--border); border-radius: 1.2em; background: var(--background); font-size: inherit; transition: border-color 0.2s; }
	button.composer, .composer:has(button) { border-color: var(--border-stronger); }
	.placeholder { font-size: 0.72em; color: var(--muted-foreground); }
	.suggestion { display: flex; flex: 1; align-items: center; gap: 0.5em; min-width: 0; height: 100%; font-size: 0.72em; color: var(--foreground); text-align: left; }
	.suggestion > span:first-child { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; animation: type-in 0.5s steps(18) both; }
	.send { display: grid; flex-shrink: 0; place-items: center; width: 2.2em; height: 2.2em; border-radius: 50%; background: var(--button); color: white; transition: scale 0.15s var(--ease-out); }
	.suggestion:hover .send { scale: 1.08; }

	.enter { animation: enter 0.36s var(--ease-out) both; }

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
	.note-below { position: relative; margin: 2em 0 0 1.6em; }
	.note-below::before { content: ''; position: absolute; bottom: calc(100% + 0.55em); left: 0.35em; width: 1px; height: 1.4em; background: var(--border-stronger); }
	.note-encrypted { z-index: 2; top: 0; left: 59.491%; translate: -50% 0; }
	.note-encrypted::after { content: ''; position: absolute; top: calc(100% + 0.5em); left: 50%; width: 1px; height: 1.6em; background: var(--border-stronger); }

	@keyframes blink { 50% { opacity: 0; } }
	@keyframes spin { to { rotate: 360deg; } }
	@keyframes enter { from { opacity: 0; translate: 0 0.4em; } }
	@keyframes type-in { from { clip-path: inset(0 100% 0 0); } }
	@keyframes busy { 0% { box-shadow: 0 0 0 0 rgb(215 153 33 / 50%); } 100% { box-shadow: 0 0 0 0.5em rgb(215 153 33 / 0%); } }
	@keyframes attention { 0%, 100% { box-shadow: 0 0 0 0 rgb(177 98 134 / 0%); } 50% { box-shadow: 0 0 0 0.25em rgb(177 98 134 / 14%); } }

	@media (prefers-reduced-motion: reduce) {
		.cursor, .approval, .enter, .dot-agent, .t-step.running::before, .p-step :global(.spin), .suggestion > span:first-child { animation: none; }
	}

	/* Narrow screens: the scene becomes a column, with each note above the part it describes. */
	@media (max-width: 700px) {
		.stage { display: grid; grid-template-columns: minmax(0, 1fr); justify-items: start; aspect-ratio: auto; margin-top: 36px; }
		.stage > * { position: static; font-size: 13px; }
		.rule-h, .note-encrypted::after { display: none; }
		.group-clients { flex-direction: column-reverse; align-items: flex-start; gap: 12px; }
		.group-terminal { width: 100%; margin-top: 28px; flex-direction: column-reverse; gap: 12px; }
		.note-below { margin: 0; }
		.note-below::before { display: none; }
		.note-encrypted { grid-area: 3 / 1; align-self: center; max-width: calc(50% - 18px); white-space: normal; translate: none; }
		.connection { grid-area: 3 / 1; position: relative; inset: auto; justify-self: center; width: 160px; height: 80px; }
		.phone { justify-self: center; width: 250px; height: 400px; }
	}
</style>
