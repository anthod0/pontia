# Pontia website

The public website at [pontia.dev](https://pontia.dev), deployed to Cloudflare Workers.

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

## Cloudflare deployment

For a local manual deployment, authenticate Wrangler outside the repository and deploy:

```sh
pnpm --dir apps/website exec wrangler login
pnpm --dir apps/website deploy
```

For CI, configure these in the deployment provider's secret store, never in repository files:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

Attach `pontia.dev` as a Worker custom domain in the Cloudflare dashboard to keep repository configuration independent of a particular account or zone.
