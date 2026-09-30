# Pontia cloud

The public website at [pontia.dev](https://pontia.dev), deployed to Cloudflare Workers.

## Development

From the repository root:

```sh
bun install --cwd apps/cloud
bun run --cwd apps/cloud dev
```

## Verification

```sh
bun run --cwd apps/cloud check
bun run --cwd apps/cloud build
bun run --cwd apps/cloud preview
```

## Cloudflare deployment

For a local manual deployment, authenticate Wrangler outside the repository and deploy:

```sh
bun x --cwd apps/cloud wrangler login
bun run --cwd apps/cloud deploy
```

For CI, configure these in the deployment provider's secret store, never in repository files:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

Attach `pontia.dev` as a Worker custom domain in the Cloudflare dashboard to keep repository configuration independent of a particular account or zone.
