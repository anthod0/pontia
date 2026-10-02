# Self-hosted edge network options

Use the deployment command from Pontia Cloud to authorize `pontia-edge init`.
Append either or both of these independent options:

- `--port 8443`: verify the public IP on 8443, then serve HTTPS/WSS on 8443.
  Other ports from 1–65535 are accepted except browser/runtime unsafe ports.
- `--acme-challenge dns-01`: issue and renew certificates through DNS TXT
  records managed by Pontia Cloud. No DNS provider credentials belong on the VPS.

For a VPS where public 80 and 443 are unavailable, append:

```sh
--port 8443 --acme-challenge dns-01
```

Open inbound TCP 8443 for both the temporary HTTP IP verification and the final
TLS service. The published endpoint is `wss://<edge-name>.edge.pontia.dev:8443/tunnel`.
The temporary listener closes as soon as Cloud confirms address verification,
before certificate issuance and service startup.

Without `--port`, IP verification uses 80 and HTTPS/WSS uses 443. Without
`--acme-challenge`, HTTP-01 uses public port 80 for issuance and renewal, even
when the service port is customized. DNS-01 alone does not change either port.
Only lowercase `http-01` and `dns-01` are accepted.

The service configuration saves the selected service port and validation method.
Repeating `init` on a registered edge succeeds with its existing identity; it
cannot change the endpoint or validation method.

DNS challenges are cleaned up by hostname and value after validation, including
failed validation. Cron deletes managed ACME TXT records older than 24 hours as
a fallback for interrupted deployments or renewals.
