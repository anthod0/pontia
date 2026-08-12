# Pontia website

The public Pontia website. It is a SvelteKit application deployed independently from the Pontia Rust server to Cloudflare Workers.

## Development

From the repository root:

```sh
pnpm --dir apps/website install
pnpm --dir apps/website dev
```

## Verification

```sh
pnpm --dir apps/website check
pnpm --dir apps/website build
pnpm --dir apps/website preview
```

All routes are prerendered by default and emitted to `build/`. Cloudflare serves the output through Workers Static Assets without a Worker script. If the website later needs server-rendered routes, replace the static adapter with the Cloudflare adapter and opt those routes out of prerendering.

## Cloudflare deployment

`wrangler.jsonc` contains only public application configuration. It intentionally does not contain an account ID, API token, secrets, or environment-specific bindings.

For a local manual deployment, authenticate Wrangler outside the repository and deploy:

```sh
pnpm --dir apps/website exec wrangler login
pnpm --dir apps/website deploy
```

For CI, configure these in the deployment provider's secret store, never in repository files:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

Attach `pontia.dev` as a Worker custom domain in the Cloudflare dashboard. Keeping the custom-domain attachment outside `wrangler.jsonc` avoids coupling repository configuration to a particular Cloudflare account or zone.

Local `.env*`, `.dev.vars*`, and `.wrangler/` files are ignored. Use Cloudflare secrets for any future sensitive runtime value:

```sh
pnpm --dir apps/website exec wrangler secret put SECRET_NAME
```
