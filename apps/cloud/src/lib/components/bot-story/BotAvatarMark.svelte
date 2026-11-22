<script lang="ts">
	import { shapes, type Shape } from '$lib/bot-stories';
	let { shape, color }: { shape: Shape; color: string } = $props();
</script>

<svg viewBox="9.9 12.4 83 83" aria-hidden="true">
	<!-- onigiri is a small triangle drawn with a thick rounded stroke, as in Dashboard. -->
	<path
		d={shapes[shape]}
		fill={color}
		stroke={shape === 'onigiri' ? color : undefined}
		stroke-width={shape === 'onigiri' ? 28 : undefined}
		stroke-linejoin="round"
	/>
	<g class="eyes" fill="#fff">
		<rect x="33" y="45" width="10" height="18" rx="5" />
		<rect x="57" y="45" width="10" height="18" rx="5" />
	</g>
</svg>

<style>
	svg { display: block; width: 100%; height: 100%; overflow: visible; }
	.eyes rect { transform-box: fill-box; transform-origin: center; animation: blink 7s linear infinite; }
	@keyframes blink { 0%, 95%, 100% { transform: scaleY(1); } 97% { transform: scaleY(0.1); } }
	@media (prefers-reduced-motion: reduce) { .eyes rect { animation: none; } }
</style>
