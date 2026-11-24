<script lang="ts">
	import { Spring, prefersReducedMotion } from 'svelte/motion';
	import { MediaQuery } from 'svelte/reactivity';
	import LockSimpleIcon from 'phosphor-svelte/lib/LockSimpleIcon';
	import { m } from '$lib/paraglide/messages';

	let { pulse }: { pulse: { id: number; from: 'terminal' | 'phone' } | null } = $props();
	const mobile = new MediaQuery('(max-width: 700px)');
	const width = $derived(mobile.current ? 160 : 335);
	const height = $derived(mobile.current ? 80 : 183);
	const offset = new Spring({ x: 0, y: 0 });
	let area: HTMLDivElement;
	let drag = $state<{ id: number; x: number; y: number } | null>(null);

	// Moving both cubic controls by 4/3 of the pull moves the midpoint by exactly the pull.
	const dx = $derived(offset.current.x * 4 / 3);
	const dy = $derived(offset.current.y * 4 / 3);
	const channel = $derived(mobile.current
		? `M80 0 C${120 + dx} ${20 + dy}, ${120 + dx} ${60 + dy}, 80 80`
		: `M0 183 C${90 + dx} ${50 + dy}, ${245 + dx} ${50 + dy}, 335 183`);
	const lockX = $derived((mobile.current ? 110 : 167.5) + offset.current.x);
	const lockY = $derived((mobile.current ? 40 : 83.25) + offset.current.y);

	function grab(event: PointerEvent & { currentTarget: HTMLButtonElement }) {
		if (drag || !event.isPrimary || event.button !== 0) return;
		event.currentTarget.setPointerCapture(event.pointerId);
		const rect = area.getBoundingClientRect();
		void offset.set(offset.current, { instant: true });
		drag = {
			id: event.pointerId,
			x: event.clientX - offset.current.x * rect.width / width,
			y: event.clientY - offset.current.y * rect.height / height
		};
	}

	function pull(event: PointerEvent) {
		if (!drag || event.pointerId !== drag.id) return;
		const rect = area.getBoundingClientRect();
		const x = (event.clientX - drag.x) * width / rect.width;
		const y = (event.clientY - drag.y) * height / rect.height;
		void offset.set({
			x: Math.max(-width / 3, Math.min(width / 3, x)),
			y: Math.max(-height / 3, Math.min(height / 3, y))
		}, { instant: true });
	}

	function reset() {
		void offset.set({ x: 0, y: 0 }, { instant: prefersReducedMotion.current });
	}

	function release(event: PointerEvent) {
		if (event.pointerId !== drag?.id) return;
		drag = null;
		reset();
	}

	function keyPull(event: KeyboardEvent) {
		if (drag || (event.key !== ' ' && event.key !== 'Enter')) return;
		event.preventDefault();
		void offset.set({ x: 0, y: height / 4 }, { instant: true });
	}

	function beginMotion(node: SVGAnimateMotionElement) {
		node.beginElement();
	}
</script>

<div class="area" bind:this={area}>
	<svg class="channel" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
		<path d={channel} />
		<circle class="anchor" cx={mobile.current ? 80 : 0} cy={mobile.current ? 0 : 183} r="4" />
		<circle class="anchor" cx={mobile.current ? 80 : 335} cy={height} r="4" />
		<circle class="packet" r="3.5"><animateMotion dur="2.6s" repeatCount="indefinite" path={channel} keyPoints="0;1" keyTimes="0;1" calcMode="spline" keySplines="0.6 0 0.4 1" /></circle>
		<circle class="packet back" r="3.5"><animateMotion dur="2.6s" begin="-1.3s" repeatCount="indefinite" path={channel} keyPoints="1;0" keyTimes="0;1" calcMode="spline" keySplines="0.6 0 0.4 1" /></circle>
		{#if pulse}
			{#key pulse.id}
				<circle class="pulse" r="6"><animateMotion {@attach beginMotion} begin="indefinite" dur="0.7s" fill="freeze" path={channel} keyPoints={pulse.from === 'terminal' ? '0;1' : '1;0'} keyTimes="0;1" calcMode="spline" keySplines="0.5 0 0.3 1" /></circle>
			{/key}
		{/if}
	</svg>
	<button type="button" class="lock" class:grabbing={drag} style:left={`${100 * lockX / width}%`} style:top={`${100 * lockY / height}%`}
		aria-label={m.remote_lock_drag()} title={m.remote_lock_drag()}
		onpointerdown={grab} onpointermove={pull} onpointerup={release} onpointercancel={release} onlostpointercapture={release}
		onkeydown={keyPull} onkeyup={(event) => { if (!drag && (event.key === ' ' || event.key === 'Enter')) reset(); }} onblur={() => { if (!drag) reset(); }}>
		{#key pulse?.id}<span class={['badge', pulse && 'sealed']}><LockSimpleIcon size={14} weight="bold" /></span>{/key}
	</button>
</div>

<style>
	.area { position: relative; width: 100%; height: 100%; }
	.channel { position: absolute; z-index: 1; inset: 0; width: 100%; height: 100%; overflow: visible; pointer-events: none; }
	.channel path { fill: none; stroke: var(--primary); stroke-width: 1.2; stroke-dasharray: 4 5; vector-effect: non-scaling-stroke; animation: flow 1.2s linear infinite; }
	.anchor { fill: var(--background); stroke: var(--primary); stroke-width: 1.2; vector-effect: non-scaling-stroke; }
	.packet { fill: var(--primary); }
	.packet.back { fill: var(--olive); }
	.pulse { fill: var(--primary); stroke: var(--background); stroke-width: 2; vector-effect: non-scaling-stroke; animation: pulse-out 0.9s ease-in both; }
	.lock { z-index: 2; position: absolute; display: grid; place-items: center; width: 44px; height: 44px; translate: -50% -50%; pointer-events: auto; padding: 0; border: 0; background: transparent; cursor: grab; touch-action: none; user-select: none; border-radius: 50%; }
	.lock.grabbing { cursor: grabbing; }
	.badge { display: grid; place-items: center; width: 2.1em; height: 2.1em; border: 1px solid var(--primary); border-radius: 50%; background: var(--background); color: var(--primary); box-shadow: 0 0 0 0.4em color-mix(in srgb, var(--primary) 10%, transparent); }
	.badge.sealed { animation: seal 0.7s var(--ease-out) 0.15s; }

	@keyframes flow { to { stroke-dashoffset: -18; } }
	@keyframes pulse-out { 0%, 70% { opacity: 1; } 100% { opacity: 0; } }
	@keyframes seal { 40% { scale: 1.18; box-shadow: 0 0 0 0.7em color-mix(in srgb, var(--primary) 18%, transparent); } }
	@media (prefers-reduced-motion: reduce) {
		.channel path, .badge.sealed { animation: none; }
		.packet, .pulse { display: none; }
	}
</style>
