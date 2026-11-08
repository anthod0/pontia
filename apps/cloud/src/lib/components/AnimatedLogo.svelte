<script lang="ts">
	type Mood = 'idle' | 'curious';
	let { class: className = '', mood = 'idle' }: { class?: string; mood?: Mood } = $props();

	// Maximum eye offset in SVG units.
	const maxX = 6;
	const maxY = 5;
	// Pointer distance (px) at which the eyes reach their maximum offset.
	const reach = 400;
	// Return to the idle glance after the pointer rests this long (ms).
	const idleDelay = 2500;

	let svg: SVGSVGElement;
	let offset = $state({ x: 0, y: 0 });
	let following = $state(false);
	let poked = $state(false);
	let pokeTimer: ReturnType<typeof setTimeout>;

	export function poke() {
		poked = false;
		clearTimeout(pokeTimer);
		// Restart the squish animation on rapid repeated clicks.
		requestAnimationFrame(() => {
			poked = true;
			pokeTimer = setTimeout(() => (poked = false), 450);
		});
	}

	$effect(() => {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

		let frame = 0;
		let idleTimer: ReturnType<typeof setTimeout>;
		let pointer = { x: 0, y: 0 };

		const update = () => {
			frame = 0;
			const box = svg.getBoundingClientRect();
			const dx = pointer.x - (box.left + box.width / 2);
			const dy = pointer.y - (box.top + box.height / 2);
			const distance = Math.hypot(dx, dy);
			if (distance === 0) {
				offset = { x: 0, y: 0 };
				return;
			}
			const strength = Math.min(distance / reach, 1);
			offset = { x: (dx / distance) * maxX * strength, y: (dy / distance) * maxY * strength };
		};

		const rest = () => {
			following = false;
			offset = { x: 0, y: 0 };
		};

		const onMove = (event: PointerEvent) => {
			pointer = { x: event.clientX, y: event.clientY };
			following = true;
			if (!frame) frame = requestAnimationFrame(update);
			clearTimeout(idleTimer);
			idleTimer = setTimeout(rest, idleDelay);
		};

		const onLeave = () => {
			clearTimeout(idleTimer);
			rest();
		};

		window.addEventListener('pointermove', onMove, { passive: true });
		document.documentElement.addEventListener('pointerleave', onLeave);
		return () => {
			window.removeEventListener('pointermove', onMove);
			document.documentElement.removeEventListener('pointerleave', onLeave);
			cancelAnimationFrame(frame);
			clearTimeout(idleTimer);
		};
	});
</script>

<svg
	bind:this={svg}
	class={['logo', className]}
	class:following
	class:curious={mood === 'curious'}
	class:poked
	onclick={poke}
	xmlns="http://www.w3.org/2000/svg"
	viewBox="9.9 12.4 83 83"
	aria-hidden="true"
>
	<g class="squish"><g class="body">
		<path
			fill="#458588"
			d="M19 38C26 23 46 22 55 30C61 35 69 30 78 32C93 36 95 62 81 74C67 86 31 86 18 72C8 61 13 47 19 38Z"
		/>
		<g class="follow" style:transform="translate({offset.x}px, {offset.y}px)">
			<g class="widen">
				<g class="gaze">
					<rect class="eye" fill="#fff" x="33" y="45" width="10" height="18" rx="5" />
					<rect class="eye" fill="#fff" x="57" y="45" width="10" height="18" rx="5" />
				</g>
			</g>
		</g>
	</g></g>
</svg>

<style>
	.logo { display: block; overflow: visible; -webkit-tap-highlight-color: transparent; }
	.squish { transform-box: view-box; transform-origin: 51px 83px; }
	.poked .squish { animation: squish 0.45s cubic-bezier(0.3, 0.7, 0.4, 1.4); }
	.widen { transform-box: view-box; transform-origin: 50px 54px; transition: transform 0.25s cubic-bezier(0.3, 0.7, 0.4, 1.4); }
	.curious .widen { transform: scale(1.18); }
	.body { transform-box: view-box; transform-origin: 51px 83px; animation: breathe 3s ease-in-out infinite; }
	.follow { transition: transform 0.35s cubic-bezier(0.2, 0.8, 0.3, 1); }
	.gaze { animation: glance 6s ease-in-out infinite; }
	.following .gaze { animation: none; }
	.eye { transform-box: fill-box; transform-origin: center; animation: blink 6s linear infinite; }
	@keyframes squish {
		0%, 100% { transform: none; }
		35% { transform: scale(1.12, 0.82); }
		70% { transform: scale(0.96, 1.06); }
	}
	@keyframes breathe { 50% { transform: scale(1.015, 1.03); } }
	@keyframes glance {
		0%, 44%, 62%, 100% { transform: translate(0, 0); }
		48%, 58% { transform: translate(-3.5px, -0.5px); }
	}
	@keyframes blink {
		0%, 28%, 32%, 75%, 79%, 81%, 85%, 100% { transform: none; }
		30%, 77%, 83% { transform: scale(1.25, 0.3); }
	}
	@media (prefers-reduced-motion: reduce) {
		.body, .gaze, .eye, .poked .squish { animation: none; }
		.follow, .widen { transition: none; }
	}
</style>
