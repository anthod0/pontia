const sourceRoot = new URL("../src/", import.meta.url);
const forbiddenRadius =
  /(?<![\w-])((?:[\w\-[\]().=&*/]+:)*!?rounded(?:-(?:[trblxyse]|ss|se|ee|es))?(?:-(?:xs|sm|md|lg|xl|2xl|3xl|4xl)|-\[[^\]\s"'`]+\])!?)(?![\w-])/g;
const violations: string[] = [];

for await (const relativePath of new Bun.Glob("**/*.{svelte,ts,js}").scan({
  cwd: sourceRoot.pathname,
  onlyFiles: true,
})) {
  if (relativePath.startsWith("lib/components/ui/")) continue;

  const source = await Bun.file(new URL(relativePath, sourceRoot)).text();
  for (const [index, line] of source.split("\n").entries()) {
    for (const match of line.matchAll(forbiddenRadius)) {
      violations.push(`${relativePath}:${index + 1}: ${match[1]}`);
    }
  }
}

if (violations.length > 0) {
  console.error(
    [
      "Use semantic radius utilities outside lib/components/ui (for example rounded-action, rounded-item, or rounded-panel).",
      ...violations,
    ].join("\n"),
  );
  process.exit(1);
}
