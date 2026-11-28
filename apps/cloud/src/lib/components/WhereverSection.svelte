<script lang="ts">
	import { onMount } from 'svelte';
	import { fly } from 'svelte/transition';
	import { prefersReducedMotion } from 'svelte/motion';
	import { m } from '$lib/paraglide/messages';
	import { localizeHref } from '$lib/paraglide/runtime';
	import { reveal } from '$lib/reveal';
	import type { Agent } from '$lib/bot-stories';
	import BotAvatarMark from './bot-story/BotAvatarMark.svelte';
	import { places, devices } from './wherever/demo';
	import { mountStage } from './wherever/stage';
	import Check from 'phosphor-svelte/lib/CheckIcon';
	import Lock from 'phosphor-svelte/lib/LockSimpleIcon';
	import Hand from 'phosphor-svelte/lib/HandIcon';
	import Arrow from 'phosphor-svelte/lib/ArrowRightIcon';
	import Again from 'phosphor-svelte/lib/ArrowCounterClockwiseIcon';
	import X from 'phosphor-svelte/lib/XIcon';
	import './wherever/section.css';

	let section: HTMLElement;
	let motion: ReturnType<typeof mountStage>;
	let active = $state(-1),
		request = $state(-1),
		lastAsked = -1;
	let stamps = $state<number[]>([]),
		celebrating = $state(false),
		announcement = $state('');
	const complete = $derived(stamps.length === places.length);
	const location = $derived(places[active]?.phrase() ?? m.wherever_away());
	const word = $derived(
		celebrating ? m.wherever_everywhere() : (places[active]?.phrase() ?? m.wherever_default()),
	);
	const marker = '\uFFFC';
	const [prefix, suffix] = $derived(m.wherever_headline({ place: marker }).split(marker));
	const heldDevice = $derived(places[active]?.browser ? m.wherever_browser() : m.wherever_phone());
	const [youPrefix, youSuffix] = $derived(m.wherever_you({ device: marker }).split(marker));
	const [progressPrefix, progressSuffix] = $derived(
		(complete ? m.wherever_complete : m.wherever_progress)({ progress: marker }).split(marker),
	);
	let wordWidth = $state<number>();
	let sessions = $state(devices.map(() => ({ state: 'running', task: 0, note: '', hop: false })));
	let timers = new Set<ReturnType<typeof setTimeout>>();
	let askTimer: ReturnType<typeof setTimeout>;
	let visible = false,
		first = true;
	function later(fn: () => void, ms: number) {
		const timer = setTimeout(() => {
			timers.delete(timer);
			fn();
		}, ms);
		timers.add(timer);
	}
	function schedule(ms: number) {
		clearTimeout(askTimer);
		if (!visible || request >= 0) return;
		askTimer = setTimeout(() => {
			const available = sessions
				.map((s, i) => (s.state === 'running' && i !== lastAsked ? i : -1))
				.filter((i) => i >= 0);
			if (!available.length) {
				schedule(800);
				return;
			}
			request = available[Math.floor(Math.random() * available.length)];
			lastAsked = request;
			sessions[request].state = 'waiting';
			announcement = requestLabel(request);
		}, ms);
	}
	function task(i: number) {
		return devices[i].tasks[sessions[i].task % devices[i].tasks.length];
	}
	function requestLabel(i: number) {
		return m.wherever_request({ agent: devices[i].agent.client, device: devices[i].name(), cmd: task(i)[1] });
	}
	function hop(i: number) {
		sessions[i].hop = true;
		later(() => (sessions[i].hop = false), 700);
	}
	function answer(approved: boolean) {
		const i = request;
		if (i < 0) return;
		const place = location,
			session = sessions[i];
		if (section.querySelector('.ask')?.contains(document.activeElement))
			section.querySelector<HTMLButtonElement>('.you')?.focus({ preventScroll: true });
		request = -1;
		announcement = approved ? m.wherever_approved({ place }) : m.wherever_denied({ place });
		motion.send(i, () => {
			hop(i);
			session.state = approved ? 'busy' : 'running';
			if (approved)
				later(() => {
					session.state = 'done';
					session.note = m.wherever_approved({ place });
					later(() => {
						session.state = 'running';
						session.task++;
					}, 2600);
				}, 1500);
		});
		if (active >= 0 && !stamps.includes(active)) {
			stamps = [...stamps, active];
			if (stamps.length === places.length) {
				celebrating = true;
				announcement = m.wherever_complete({ progress: m.wherever_all_places() });
				devices.forEach((_, j) => later(() => hop(j), j * 120));
				later(() => (celebrating = false), 4200);
			}
		}
		schedule(approved ? 3600 : 2600);
	}
	onMount(() => {
		motion = mountStage(
			section,
			(i) => (active = i),
			(i) => (announcement = m.wherever_settled({ place: places[i]?.phrase() ?? m.wherever_away() })),
			(v) => {
				visible = v;
				schedule(first ? 1600 : 900);
				if (v) first = false;
			},
		);
		return () => {
			motion.destroy();
			clearTimeout(askTimer);
			for (const timer of timers) clearTimeout(timer);
		};
	});
	function shortcut(e: KeyboardEvent) {
		if (
			request >= 0 &&
			e.key.toLowerCase() === 'a' &&
			!e.ctrlKey &&
			!e.metaKey &&
			!e.altKey &&
			section.contains(document.activeElement)
		) {
			e.preventDefault();
			answer(true);
		}
	}
</script>

<svelte:window onkeydown={shortcut} />
{#snippet logo(agent: Agent)}
	<img class:codex-light={agent.darkLogo} src={agent.logo} alt="" width="14" height="14" />
	{#if agent.darkLogo}<img class="codex-dark" src={agent.darkLogo} alt="" width="14" height="14" />{/if}
{/snippet}
<section bind:this={section} class="wherever page-width" aria-labelledby="wherever-title" {@attach reveal()}>
	<h2 id="wherever-title" class="section-title reveal">
		{prefix}<button type="button" class="slot" aria-label={m.wherever_next({ place: word })}
			><span class="slot-track" style:width={wordWidth ? wordWidth + 'px' : undefined}
				><span class="word-size" aria-hidden="true" bind:offsetWidth={wordWidth}>{word}</span
				>{#key word}<span
						class="word"
						class:accent={active >= 0 || celebrating}
						in:fly={{
							y: prefersReducedMotion.current ? 0 : '100%',
							duration: prefersReducedMotion.current ? 0 : 450,
						}}
						out:fly={{
							y: prefersReducedMotion.current ? 0 : '-100%',
							duration: prefersReducedMotion.current ? 0 : 450,
						}}>{word}</span
					>{/key}</span
			></button
		>{suffix}
	</h2>
	<p class="sub reveal" style="--i:1">{m.home_closing_text()}</p>
	<div
		class="stage reveal"
		style="--i:2"
		role="group"
		aria-label={m.wherever_stage()}
		aria-describedby="wherever-help"
	>
		<div class="field" aria-hidden="true"></div>
		<span class="tray-label">{m.wherever_devices()}</span>
		<svg class="ropes" aria-hidden="true"
			>{#each devices as _, i}<path class="rope" class:waiting={sessions[i].state === 'waiting'} /><circle
					class="packet down"
					r="3"
				/><circle class="packet up" r="3" /><circle class="pulse" r="6" style="display:none" />{/each}</svg
		>
		<div class="machines">
			{#each devices as d, i}
				{@const session = sessions[i]}{@const [title, cmd] = task(i)}
				<div class={'machine ' + session.state} class:got={session.hop}>
					<div class="m-head">
						<span class="bot" class:hop={session.hop}><BotAvatarMark shape={d.shape} color={d.color} /></span
						><span class="m-name"
							><b><span class="long">{d.name()}</span><span class="short">{d.short()}</span></b><small
								><d.icon size="1em" /><span>{d.location()}</span></small
							></span
						><span class="dot"></span>
					</div>
					<div class="m-session">{@render logo(d.agent)}<span>{title()}</span></div>
					<div class="m-status">
						{#if session.state === 'waiting'}
							<Hand weight="bold" class="icon" />
							<span class="long">{m.wherever_waiting({ cmd })}</span><span class="short">{m.wherever_asking()}</span>
						{:else if session.state === 'done'}
							<Check weight="bold" class="icon" />
							<span class="long">{session.note}</span><span class="short">{m.wherever_approved_short()}</span>
						{:else}
							<span class="spin"></span>
							<span class="long">{session.state === 'busy' ? m.wherever_busy({ cmd }) : m.wherever_working()}</span>
							<span class="short">{session.state === 'busy' ? m.wherever_running() : m.story_state_running()}</span>
						{/if}
					</div>
				</div>
			{/each}
		</div>
		<a class="cta" class:cheer={complete} href="https://app.pontia.dev"
			><span class="cta-note">{m.wherever_turn()}</span>{m.get_started()}<Arrow
				class="icon"
				weight="bold"
			/></a
		>
		{#each devices as _, i}<span
				class="lock"
				aria-hidden="true"
				class:waiting={sessions[i].state === 'waiting'}
				class:seal={sessions[i].hop}><Lock size="1em" weight="bold" /></span
			>{/each}
		{#each places as p, i}<button
				type="button"
				class="place"
				class:stamped={stamps.includes(i)}
				aria-label={m.wherever_go({ name: p.name() })}
				><span class="place-ring"
					><p.icon size="1em" /><span class="stamp"><Check size="1em" weight="bold" /></span></span
				><span>{p.name()}</span></button
			>{/each}
		<button
			type="button"
			class="you"
			aria-label={m.wherever_device_place({ device: heldDevice, place: location })}
			class:browser={places[active]?.browser}
			class:asked={request >= 0}
			aria-describedby="wherever-help"
		>
			<span class="device" aria-hidden="true"
				><span class="bar"><b></b><b></b><b></b></span><span class="screen"
					><i></i><i></i><i></i><span class="badge"></span></span
				></span
			>
			<span class="you-label">{youPrefix}<em>{heldDevice}</em>{youSuffix}</span>
		</button>
		<div class="hint" aria-hidden="true">
			<Hand class="icon" /><span class="long">{m.wherever_hint()}</span><span class="short"
				>{m.wherever_drag()}</span
			>
		</div>
		<div
			class="ask"
			role="dialog"
			aria-label={request >= 0 ? requestLabel(request) : undefined}
			hidden={request < 0}
		>
			{#if request >= 0}<div class="ask-from">{@render logo(devices[request].agent)}</div>
				<code class="ask-cmd">{task(request)[1]}</code>
				<div class="ask-actions">
					<button type="button" class="deny" aria-label={m.remote_demo_deny()} onclick={() => answer(false)}
						><X weight="bold" class="icon" /></button
					><button type="button" class="approve" onclick={() => answer(true)}
						>{m.remote_demo_approve()}<kbd>A</kbd></button
					>
				</div>{/if}
		</div>
		<p class="sr-only" id="wherever-help">{m.wherever_help()}</p>
	</div>
	<div class="passport reveal" style="--i:3" class:complete>
		<span class="stamps" aria-hidden="true"
			>{#each places as p, i}<span class:on={stamps.includes(i)} title={p.name()}
					><p.icon size="1em" weight="bold" /></span
				>{/each}</span
		>
		<span
			>{progressPrefix}<b
				>{complete ? m.wherever_all_places() : m.wherever_fraction({ count: stamps.length })}</b
			>{progressSuffix}</span
		><button
			type="button"
			class="again"
			onclick={() => {
				stamps = [];
				celebrating = false;
			}}><Again size="1em" weight="bold" />{m.wherever_again()}</button
		>
	</div>
	<p class="guide reveal" style="--i:4">
		{m.wherever_guide()} <a href={localizeHref('/docs/getting-started')}>{m.setup_guide()}</a>
		<span aria-hidden="true">·</span> <a href={localizeHref('/docs/faq')}>{m.docs_faq()}</a>
	</p>
	<p class="sr-only" aria-live="polite">{announcement}</p>
</section>
