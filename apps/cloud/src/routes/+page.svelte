<script lang="ts">
	import ArrowUpRightIcon from 'phosphor-svelte/lib/ArrowUpRightIcon';
	import MonitorIcon from 'phosphor-svelte/lib/MonitorIcon';
	import ShieldCheckIcon from 'phosphor-svelte/lib/ShieldCheckIcon';
	import TerminalWindowIcon from 'phosphor-svelte/lib/TerminalWindowIcon';
	import HeroSection from '$lib/components/HeroSection.svelte';

	const repository = 'https://github.com/anthod0/pontia';
</script>

<svelte:head>
	<title>Pontia — Your device. Your agent cloud.</title>
	<meta name="description" content="Run agents on your device. Control them from anywhere." />
	<meta property="og:title" content="Pontia — Your device. Your agent cloud." />
	<meta property="og:description" content="Run agents on your device. Control them from anywhere." />
	<meta property="og:type" content="website" />
	<meta name="twitter:card" content="summary" />
</svelte:head>

<a class="skip-link" href="#main">Skip to content</a>

<header class="site-header page-width">
	<a class="brand" href="/" aria-label="Pontia home"><img src="/logo.svg" alt="" /><span>Pontia</span></a>
	<nav aria-label="Primary navigation">
		<a class="nav-link" href="/docs">Docs</a>
		<a class="nav-link source-link" href={repository}>GitHub<ArrowUpRightIcon size={13} /></a>
		<a class="nav-link sign-in-link" href="/login">Sign in</a>
		<a class="button button-small" href="https://app.pontia.dev">Open Dashboard</a>
	</nav>
</header>

<main id="main">
	<HeroSection />


	<section id="features" class="features page-width section-block" aria-labelledby="features-title">
		<div class="section-intro">
			<h2 id="features-title">Control your agents.<br />From anywhere.</h2>
			<p>Start work on your device, then monitor progress<br class="desktop-break" /> and take action from the remote dashboard<br class="desktop-break" /> wherever you are.</p>
		</div>
		<div class="feature-grid">
			<article class="feature">
				<div class="feature-top"><MonitorIcon size={25} weight="light" /><span>01</span></div>
				<h3>Remote session control.</h3>
				<p>Create, view, continue, interrupt, and manage coding agent sessions from your browser—even when you’re away from your desk.</p>
				<div class="feature-detail"><span class="surface-tag">Device</span><span>↔</span><span class="surface-tag">Web</span></div>
			</article>
			<article class="feature">
				<div class="feature-top"><ShieldCheckIcon size={25} weight="light" /><span>02</span></div>
				<h3>Encrypted end to end.</h3>
				<p>Dashboard traffic is encrypted between your browser and your device. The Edge relays ciphertext without seeing your session data.</p>
				<div class="feature-detail"><span class="status-dot"></span>Private by design</div>
			</article>
			<article class="feature">
				<div class="feature-top"><TerminalWindowIcon size={25} weight="light" /><span>03</span></div>
				<h3>Execution stays local.</h3>
				<p>Your agents keep running with the development tools and project files on your device. Pontia provides access, not the execution environment.</p>
				<div class="feature-detail"><span class="status-dot ochre"></span>Local-first execution</div>
			</article>
		</div>
	</section>

</main>

<footer class="site-footer page-width">
	<a class="brand footer-brand" href="/" aria-label="Pontia home"><img src="/logo.svg" alt="" /><span>Pontia</span></a>
	<p>Your device. Your agent cloud.</p>
	<nav aria-label="Footer navigation"><a href={repository}>GitHub<ArrowUpRightIcon size={12} /></a><a href="/docs">Documentation</a><a href={`${repository}/blob/main/LICENSE`}>Apache 2.0</a></nav>
</footer>
