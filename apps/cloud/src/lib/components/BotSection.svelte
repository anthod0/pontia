<script lang="ts">
	import { onMount } from 'svelte';
	import CheckIcon from 'phosphor-svelte/lib/CheckIcon';
	import { reveal } from '$lib/reveal';
	import { m } from '$lib/paraglide/messages';

	// Avatar shapes and colours mirror apps/dashboard/src/components/bots/avatar.ts.
	const shapes = {
		classic: 'M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z',
		mochi: 'M51 30C72 30 84 44 88 62C91 74 92 84 80 84H22C10 84 11 74 14 62C18 44 30 30 51 30Z',
		cloud: 'M18 72C8 62 12 46 22 42C22 30 36 26 44 32C48 24 62 24 66 32C74 26 88 32 84 44C94 50 92 66 82 74C68 86 32 86 18 72Z',
		drop: 'M51 18C60 32 87 42 87 64C87 81 70 84 51 84C32 84 15 81 15 64C15 42 42 32 51 18Z',
		blob: 'M24 34C34 22 52 30 62 26C76 20 92 34 86 52C82 64 92 76 78 82C64 88 50 78 36 84C20 90 10 76 14 62C17 52 15 44 24 34Z',
		ghost: 'M18 50C18 32 32 24 51 24C70 24 84 32 84 50V76Q73 90 62 79Q51 90 40 79Q29 90 18 76Z',
		onigiri: 'M51.0 32.0Q68.6 50.3 79.0 72.0Q51.0 76.0 23.0 72.0Q33.4 50.3 51.0 32.0Z',
		egg: 'M51 18C70 18 84 44 84 62C84 78 70 86 51 86C32 86 18 78 18 62C18 44 32 18 51 18Z',
		pudding: 'M30 28H72C78 28 80 31 81 36L88 76C89 82 86 84 80 84H22C16 84 13 82 14 76L21 36C22 31 24 28 30 28Z'
	};

	type Status = { text: () => string; state: 'working' | 'done' };
	const cast: { shape: keyof typeof shapes; color: string; size: number; status?: Status }[] = [
		{ shape: 'ghost', color: '#b16286', size: 76 },
		{ shape: 'egg', color: '#d79921', size: 92, status: { text: m.bots_status_video, state: 'working' } },
		{ shape: 'onigiri', color: '#689d6a', size: 70 },
		{ shape: 'classic', color: '#458588', size: 132, status: { text: m.bots_status_tests, state: 'working' } },
		{ shape: 'pudding', color: '#d65d0e', size: 84 },
		{ shape: 'cloud', color: '#5f6fa3', size: 98, status: { text: m.bots_status_done, state: 'done' } },
		{ shape: 'drop', color: '#c45a76', size: 72 },
		{ shape: 'mochi', color: '#98971a', size: 88 },
		{ shape: 'blob', color: '#9c6b3c', size: 74 }
	];

	// Every Bot glances toward the pointer, like the Dashboard mascot.
	let stage: HTMLDivElement;
	let gazeX = $state(0);
	let gazeY = $state(0);

	onMount(() => {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		let frame = 0;
		let pointerX = 0;
		let pointerY = 0;
		const update = () => {
			frame = 0;
			const rect = stage.getBoundingClientRect();
			const dx = pointerX - (rect.left + rect.width / 2);
			const dy = pointerY - (rect.top + rect.height / 2);
			gazeX = Math.max(-1, Math.min(1, dx / (rect.width / 2))) * 4;
			gazeY = Math.max(-1, Math.min(1, dy / rect.height)) * 3;
		};
		const move = (event: PointerEvent) => {
			pointerX = event.clientX;
			pointerY = event.clientY;
			if (!frame) frame = requestAnimationFrame(update);
		};
		window.addEventListener('pointermove', move, { passive: true });
		return () => {
			window.removeEventListener('pointermove', move);
			if (frame) cancelAnimationFrame(frame);
		};
	});
</script>

<section id="bots" class="bots section-block" aria-labelledby="bots-title" {@attach reveal()}>
	<div class="intro page-width reveal">
		<h2 id="bots-title">{m.bots_title()}</h2>
		<p>{m.bots_intro()}</p>
	</div>

	<div class="stage" bind:this={stage} aria-hidden="true">
		{#each cast as bot, index}
			<div class="bot reveal" style={`--i: ${index % 5}; --size: ${bot.size}px; --phase: ${index * -0.7}s; --blink: ${index * -1.3}s`}>
				{#if bot.status}
					<span class={['status', bot.status.state]}>
						{#if bot.status.state === 'done'}<CheckIcon size={11} weight="bold" />{:else}<span class="dot"></span>{/if}
						{bot.status.text()}
					</span>
				{/if}
				<svg viewBox="9.9 12.4 83 83">
					<path d={shapes[bot.shape]} fill={bot.color}
						stroke={bot.shape === 'onigiri' ? bot.color : undefined}
						stroke-width={bot.shape === 'onigiri' ? 28 : undefined} stroke-linejoin="round" />
					<g class="eyes" fill="#fff" style:transform={`translate(${gazeX}px, ${gazeY}px)`}>
						<rect x="33" y="45" width="10" height="18" rx="5" />
						<rect x="57" y="45" width="10" height="18" rx="5" />
					</g>
				</svg>
				<span class="shadow"></span>
			</div>
		{/each}
	</div>
</section>

<style>
	/* The hero already ends with generous padding, so this section sits close beneath it. */
	.bots { overflow: hidden; padding-block: 24px 8px; }
	.intro { text-align: center; }
	.intro h2 { font-size: clamp(32px, 4.4vw, 48px); letter-spacing: -0.045em; }
	.intro p { max-width: 460px; margin: 14px auto 0; font-size: 15px; line-height: 1.7; color: var(--muted-foreground); text-wrap: balance; word-break: auto-phrase; }

	/* A loose crowd: sizes vary and the line dips toward the larger Bot in the middle. */
	.stage { display: flex; justify-content: center; align-items: flex-end; gap: clamp(8px, 2vw, 28px); margin-top: 72px; padding-inline: 24px; }
	.bot { position: relative; display: flex; flex-direction: column; align-items: center; flex-shrink: 0; }
	.bot svg { width: var(--size); height: var(--size); overflow: visible; animation: bob 4.2s ease-in-out var(--phase) infinite; }
	.eyes { transition: transform 160ms ease-out; }
	.eyes rect { transform-box: fill-box; transform-origin: center; animation: blink 7s linear var(--blink) infinite; }
	.shadow { width: calc(var(--size) * 0.55); height: 6px; margin-top: 2px; border-radius: 50%; background: var(--heading); opacity: 0.08; animation: shadow 4.2s ease-in-out var(--phase) infinite; }

	.status { position: absolute; bottom: calc(100% + 6px); left: 50%; display: inline-flex; align-items: center; gap: 6px; padding: 5px 10px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); box-shadow: 0 8px 20px -14px rgb(60 56 54 / 45%); font-size: 11px; font-weight: 500; color: var(--foreground); white-space: nowrap; transform: translateX(-50%); }
	.status.done { color: var(--coverage-supported); }
	.dot { width: 6px; height: 6px; border-radius: 50%; background: var(--color-busy); animation: pulse 1.6s ease-out infinite; }

	@keyframes bob { 0%, 100% { transform: translateY(0); } 50% { transform: translateY(-8px); } }
	@keyframes shadow { 0%, 100% { transform: scaleX(1); opacity: 0.08; } 50% { transform: scaleX(0.82); opacity: 0.05; } }
	@keyframes blink { 0%, 95%, 100% { transform: scaleY(1); } 97% { transform: scaleY(0.1); } }
	@keyframes pulse { 0% { box-shadow: 0 0 0 0 rgb(215 153 33 / 50%); } 100% { box-shadow: 0 0 0 6px rgb(215 153 33 / 0%); } }

	@media (prefers-reduced-motion: reduce) {
		.bot svg, .shadow, .eyes rect, .dot { animation: none; }
	}
	/* Narrow screens keep the middle of the crowd, which carries every status. */
	@media (max-width: 900px) {
		.bot:nth-child(1), .bot:nth-child(9) { display: none; }
	}
	@media (max-width: 600px) {
		.bots { padding-block: 8px 0; }
		.stage { gap: 6px; margin-top: 56px; padding-inline: 12px; }
		/* Two statuses fit side by side; the third would overlap. */
		.bot:nth-child(2) .status { display: none; }
		.bot svg { width: calc(var(--size) * 0.62); height: calc(var(--size) * 0.62); }
		.shadow { width: calc(var(--size) * 0.34); }
		.bot:nth-child(3), .bot:nth-child(7) { display: none; }
		.status { font-size: 10px; padding: 4px 8px; }
	}
</style>
