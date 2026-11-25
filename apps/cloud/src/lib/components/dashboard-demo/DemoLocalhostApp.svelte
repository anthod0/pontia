<script lang="ts">
	/**
	 * A stand-in for the visitor's own app on localhost: the Atlas web app from the Bot stories.
	 * It keeps its own light palette, as a real app would, and lays out for the viewport width it is given.
	 */
	let { path, mobile, onnavigate }: { path: string; mobile: boolean; onnavigate: (path: string) => void } = $props();

	const projects = [
		{ name: 'Website', meta: 'Updated 2h ago', tone: '#458588' },
		{ name: 'Billing', meta: 'Updated 5h ago', tone: '#d79921' },
		{ name: 'Mobile app', meta: 'Updated yesterday', tone: '#b16286' },
		{ name: 'Docs', meta: 'Updated yesterday', tone: '#689d6a' },
		{ name: 'Infra', meta: 'Updated Nov 21', tone: '#98971a' },
		{ name: 'Search', meta: 'Updated Nov 20', tone: '#7c6f64' }
	];
	const commands = [
		{ label: 'Go to Projects', hint: 'G P', to: '/projects' },
		{ label: 'Go to Settings', hint: 'G S', to: '/settings' },
		{ label: 'New project', hint: 'N' },
		{ label: 'Invite a teammate' }
	];
	let menu = $state(false);
	let active = $state(0);
	let search: HTMLButtonElement;

	function closeMenu() {
		menu = false;
		search.focus({ preventScroll: true });
	}
	const route = $derived(path.replace(/\/$/, '') || '/');

	function keydown(event: KeyboardEvent) {
		if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
			event.preventDefault();
			if (menu) closeMenu();
			else menu = true;
		} else if (menu && event.key === 'Escape') {
			event.preventDefault();
			event.stopPropagation();
			closeMenu();
		}
	}

	function run(command: (typeof commands)[number]) {
		closeMenu();
		if (command.to) onnavigate(command.to);
	}
</script>

<div class="atlas" class:mobile {@attach (node) => {
	const demo = node.closest<HTMLElement>('.dashboard-demo')!;
	demo.addEventListener('keydown', keydown, true);
	return () => demo.removeEventListener('keydown', keydown, true);
}}>
	<header>
		<span class="logo">
			<svg viewBox="0 0 20 20" aria-hidden="true"><rect width="20" height="20" rx="5" fill="#d79921" /><path d="M5.2 14.2 10 5.6l4.8 8.6h-3.1L10 11.1l-1.7 3.1Z" fill="#fff" /></svg>
			Atlas
		</span>
		{#if !mobile}
			<nav>
				<button type="button" class:current={route === '/projects'} onclick={() => onnavigate('/projects')}>Projects</button>
				<button type="button" class:current={route === '/settings'} onclick={() => onnavigate('/settings')}>Settings</button>
			</nav>
		{/if}
		<button type="button" bind:this={search} class="search" aria-label="Search commands" onclick={() => (menu = true)}>{#if !mobile}Search…{/if}<kbd>⌘K</kbd></button>
		<span class="me" aria-hidden="true">AL</span>
	</header>

	<main>
		{#if route === '/projects'}
			<div class="title"><b>Projects</b><span class="primary">New project</span></div>
			<div class="grid">
				{#each projects as project}
					<div class="card"><i style:background={project.tone}></i><b>{project.name}</b><span>{project.meta}</span></div>
				{/each}
			</div>
			<p class="more">Showing 6 of 24 · <span class="link">Next page →</span></p>
		{:else if route === '/settings'}
			<div class="title"><b>Settings</b></div>
			<section class="panel"><b>Profile</b><span class="field">Ada Lovelace</span><span class="field">ada@northwind.dev</span></section>
			<section class="panel"><b>Notifications</b><span class="toggle">Email me about deploys <i class="on"></i></span><span class="toggle">Push alerts <i></i></span></section>
			<section class="panel danger"><b>Danger zone</b><span>Delete this account and all projects.</span></section>
		{:else}
			<div class="missing"><b>404</b><span>Cannot GET {route}</span></div>
		{/if}
	</main>

	{#if menu}
		<button type="button" class="scrim" aria-label="Close command menu" onclick={closeMenu}></button>
		<div class="menu" role="dialog" aria-label="Command menu">
			<span class="query">Type a command or search…<i class="caret"></i></span>
			<span class="group">Navigation</span>
			{#each commands as command, index}
				<button type="button"  {@attach node => { if (index === 0) node.focus({ preventScroll: true }); }} class:active={index === active} onmouseenter={() => (active = index)} onclick={() => run(command)}>
					{command.label}{#if command.hint}<kbd>{command.hint}</kbd>{/if}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.atlas { position: relative; display: flex; flex-direction: column; min-height: 100%; background: #fbfaf7; color: #504945; font: 14px/1.45 system-ui, -apple-system, 'Segoe UI', sans-serif; letter-spacing: normal; text-align: left; }
	.atlas button { font: inherit; color: inherit; cursor: pointer; }
	header { display: flex; align-items: center; gap: 28px; height: 56px; padding: 0 28px; border-bottom: 1px solid #ebe6da; background: #fffefb; }
	.logo { display: inline-flex; align-items: center; gap: 8px; color: #282828; font-size: 15px; font-weight: 650; }
	.logo svg { width: 22px; height: 22px; }
	nav { display: flex; gap: 6px; }
	nav button { padding: 6px 10px; border-radius: 6px; color: #7c6f64; }
	nav button.current { background: #f2eee4; color: #282828; font-weight: 500; }
	.search { display: inline-flex; align-items: center; gap: 24px; margin-left: auto; height: 32px; padding: 0 6px 0 12px; border: 1px solid #e3ddd0; border-radius: 8px; background: #fff; color: #a89984 !important; }
	kbd { padding: 1px 6px; border: 1px solid #e3ddd0; border-radius: 5px; background: #faf8f3; color: #7c6f64; font: 11px/1.6 ui-monospace, monospace; }
	.me { display: grid; place-items: center; width: 30px; height: 30px; border-radius: 50%; background: #407e81; color: #fff; font-size: 11px; font-weight: 600; }
	main { flex: 1; padding: 28px; }
	.title { display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px; }
	.title b { color: #282828; font-size: 22px; font-weight: 650; letter-spacing: -0.01em; }
	.primary { padding: 7px 12px; border-radius: 7px; background: #407e81; color: #fff; font-size: 13px; font-weight: 500; }
	.grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 14px; }
	.card { display: grid; gap: 4px; padding: 16px; border: 1px solid #ebe6da; border-radius: 10px; background: #fff; }
	.card i { width: 26px; height: 26px; margin-bottom: 8px; border-radius: 7px; opacity: 0.85; }
	.card b { color: #282828; font-weight: 600; }
	.card span, .more { color: #928374; font-size: 12.5px; }
	.more { margin-top: 16px; }
	.link { color: #407e81; font-weight: 500; }
	.panel { display: grid; gap: 8px; max-width: 560px; margin-bottom: 14px; padding: 16px; border: 1px solid #ebe6da; border-radius: 10px; background: #fff; }
	.panel b { color: #282828; }
	.field { padding: 7px 10px; border: 1px solid #e3ddd0; border-radius: 7px; color: #282828; }
	.toggle { display: flex; align-items: center; justify-content: space-between; }
	.toggle i { width: 30px; height: 18px; border-radius: 9px; background: #e3ddd0; }
	.toggle i.on { background: #407e81; }
	.danger { border-color: #f0c8c4; }
	.danger span { color: #9d0006; }
	.missing { display: grid; place-items: center; gap: 4px; padding-top: 80px; color: #928374; }
	.missing b { color: #282828; font-size: 40px; }
	.scrim { position: absolute; inset: 0; border: 0; background: rgb(40 40 40 / 28%); }
	.menu { position: absolute; top: 96px; left: 50%; display: grid; width: min(520px, calc(100% - 32px)); padding: 6px; transform: translateX(-50%); border: 1px solid #e3ddd0; border-radius: 12px; background: #fffefb; box-shadow: 0 24px 60px -20px rgb(40 40 40 / 45%); }
	.query { display: flex; align-items: center; height: 44px; padding: 0 12px; margin-bottom: 4px; border-bottom: 1px solid #efebe2; color: #a89984; font-size: 15px; }
	.caret { width: 1px; height: 18px; margin-left: 2px; background: #282828; }
	.group { padding: 6px 10px 4px; color: #a89984; font-size: 11.5px; font-weight: 500; }
	.menu button { display: flex; align-items: center; justify-content: space-between; height: 38px; padding: 0 10px; border-radius: 7px; text-align: left; }
	.menu button.active { background: #f2eee4; color: #282828; }
	@media (prefers-reduced-motion: no-preference) { .caret { animation: blink 1s step-end infinite; } }
	@keyframes blink { 50% { opacity: 0; } }

	.mobile header { gap: 12px; height: 52px; padding: 0 16px; }
	.mobile .search { gap: 0; padding: 0 6px; }
	.mobile main { padding: 18px 16px; }
	.mobile .grid { grid-template-columns: minmax(0, 1fr); gap: 10px; }
	.mobile .card { grid-template-columns: auto 1fr; column-gap: 12px; padding: 12px; }
	.mobile .card i { grid-row: span 2; margin: 0; }
	.mobile .menu { top: 64px; }
</style>
