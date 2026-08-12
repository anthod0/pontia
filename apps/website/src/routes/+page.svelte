<svelte:head>
	<title>Pontia — Control plane for coding agents</title>
	<meta
		name="description"
		content="Pontia keeps coding-agent sessions alive, visible, controllable, and fixable."
	/>
</svelte:head>

<main class="mx-auto flex min-h-svh max-w-5xl flex-col px-6 py-8 sm:px-10">
	<nav class="flex items-center" aria-label="Primary navigation">
		<a class="flex items-center gap-3 font-semibold" href="/" aria-label="Pontia home">
			<img class="h-9 w-9" src="/logo.svg" alt="" />
			<span>Pontia</span>
		</a>
	</nav>

	<section class="flex flex-1 flex-col items-start justify-center py-24 sm:py-32">
		<h1 class="max-w-4xl text-5xl leading-[1.05] font-semibold tracking-tight sm:text-7xl">
			Keep coding agents alive, visible, and under control.
		</h1>
		<p class="mt-7 max-w-2xl text-lg leading-8 text-zinc-600 sm:text-xl dark:text-zinc-400">
			Pontia is an experimental control plane for long-lived coding-agent sessions, built around the
			real agent TUI.
		</p>
		<div class="mt-10 flex flex-wrap gap-3">
			<a
				class="rounded-full bg-zinc-950 px-5 py-3 text-sm font-medium text-white transition hover:bg-violet-600 dark:bg-white dark:text-zinc-950 dark:hover:bg-violet-400"
				href="https://github.com/anthod0/pontia#run-locally-from-source"
			>Get started</a>
			<a
				class="inline-flex items-center gap-2 rounded-full border border-zinc-300 px-5 py-3 text-sm font-medium transition hover:border-violet-500 hover:text-violet-600 dark:border-zinc-700 dark:hover:border-violet-400 dark:hover:text-violet-400"
				href="https://github.com/anthod0/pontia"
			>
				<svg class="size-4" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
					<path d="M12 .7a11.5 11.5 0 0 0-3.64 22.41c.58.11.79-.25.79-.56v-2.23c-3.22.7-3.9-1.37-3.9-1.37-.53-1.34-1.29-1.7-1.29-1.7-1.05-.72.08-.71.08-.71 1.17.08 1.78 1.2 1.78 1.2 1.04 1.78 2.72 1.27 3.39.97.1-.75.4-1.27.74-1.56-2.57-.3-5.27-1.29-5.27-5.68 0-1.26.45-2.28 1.19-3.09-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.16 1.18A10.96 10.96 0 0 1 12 6.13c.98 0 1.94.13 2.85.38 2.2-1.49 3.16-1.18 3.16-1.18.63 1.59.23 2.76.11 3.05.74.81 1.19 1.83 1.19 3.09 0 4.4-2.71 5.38-5.29 5.67.42.36.79 1.07.79 2.16v3.25c0 .31.21.68.8.56A11.5 11.5 0 0 0 12 .7Z" />
				</svg>
				View source
			</a>
		</div>
	</section>
</main>
