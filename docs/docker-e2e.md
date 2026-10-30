# Docker E2E Guide

Use the prepared local environment to validate Cloud, Public Dashboard, edge, pontiad, and real Agent calls from the current checkout. Certificates, OAuth application settings, and existing test login credentials are reused automatically; routine validation requires no setup changes.

Each run starts with fresh containers and databases and automatically provisions a public edge for the test user. Remove all run data when validation ends, including Sessions, client conversations, Workspaces, and background services.

## 1. Start the environment

Run these commands from the root of the checkout you want to validate:

```bash
./scripts/docker-e2e build
./scripts/docker-e2e check
./scripts/docker-e2e up
```

The build includes uncommitted source changes and new files in the current checkout. If certificate or credential checks fail, report the specific blocker. Do not reinitialize the environment or copy authentication files from your everyday account.

Only one run may use a host at a time, including runs from different worktrees. If startup reports an existing run, identify the task that owns it first. Only after that task has finished may you clean up with `./scripts/docker-e2e down` and retry.

## 2. Open the container browser and sign in

Open this URL in the host browser:

```text
http://127.0.0.1:18179/vnc.html?autoconnect=1&resize=scale
```

This page provides access to the browser inside the container. Agents should use `computer-use` to operate it. For browser automation, you can also connect to CDP at `127.0.0.1:9222` inside the container through a temporary `docker exec` forwarder. Close the forwarder when finished.

Access the following URLs only in the **container browser**:

| URL | Purpose |
| --- | --- |
| `https://pontia.dev` | Cloud sign-in and registration for this run |
| `https://app.pontia.dev` | Public Dashboard for this run |
| `http://localhost:8080/dashboard/` | Local Dashboard for this run |

Opening `pontia.dev` or `app.pontia.dev` directly on the host still reaches production.

Open `https://pontia.dev` in the container browser and sign in with GitHub or Google. Existing external provider login credentials are reused, but each run still requires an OAuth flow to create the local user and login session. Complete any required reauthentication or MFA in the container browser.

## 3. Sign in and register the device

Startup provisions the public edge `docker-e2e`; no deployment ticket or manual edge registration is needed. Run:

```bash
./scripts/docker-e2e login-device
```

The command displays a device login URL and code, then waits for approval. Open the URL in the container browser, check the code, and approve it. The command then registers the device, restarts the device service, waits for it to become ready, and prints the Dashboard URL.

Open the printed Dashboard URL in the container browser. Sign in and register a new device for each run; do not reuse a device ID or Dashboard URL from a previous run. If the command fails, resolve the error before retrying.

## 4. Validate real Agent calls

Pi and Codex are installed and enabled automatically, and existing test credentials are imported into the device. If a client needs authentication, use its own login flow inside the device container:

```bash
./scripts/docker-e2e compose exec device bash
# Run the following commands inside the device container:
cd /home/e2e/workspace
tmux new -s e2e
# Run pi or codex and complete any required authentication.
```

Select `/home/e2e/workspace` in Public Dashboard and exercise the shortest complete user flow relevant to the change:

1. Create a real Agent Session and send a task, such as creating a file with a marker unique to this run.
2. Observe live output and the final Turn state.
3. Test follow-up input, interruption, approvals, or bidirectional Pi TUI synchronization as relevant to the change.
4. Confirm that the Dashboard results match the actual files and client execution results inside the container.

Inspect the results from the host:

```bash
./scripts/docker-e2e compose exec -T device ls -la /home/e2e/workspace
./scripts/docker-e2e compose logs --tail=200 cloud edge device
```

Client hook logs are stored in `/home/e2e/.pontia/state/` on the device. Keep Codex configured with `workspace-write` and `on-request`. Report sandbox failures as blockers; do not disable isolation to make a test pass.

To validate Local Dashboard, securely read this run's `external_api_token` from `/home/e2e/.pontia/config.toml` on the device and enter it on the login screen. In automation, pass the value only within the process and never print it to logs. Public Dashboard does not require this token.

Healthy containers, HTTP 200 responses, connected tunnels, or authenticated clients alone do not establish an E2E pass. Before finishing, save sanitized evidence recording the source revision and uncommitted changes, client versions, model, Session ID, actions, and expected and actual results. Do not export credentials, cookies, or complete databases as evidence.

## 5. Shut down and clean up

Whether validation passes, fails, or is blocked, run this command before finishing:

```bash
./scripts/docker-e2e down
```

This preserves refreshed login credentials for reuse and removes the run's containers, networks, databases, Sessions, client conversations, Workspaces, and browser runtime data. Use the same command to clean up after a forced termination or host interruption.

Confirm that all three commands below return no output before ending the task:

```bash
docker ps -aq --filter label=com.docker.compose.project=pontia-e2e-docker
docker volume ls -q --filter label=com.docker.compose.project=pontia-e2e-docker
docker network ls -q --filter label=com.docker.compose.project=pontia-e2e-docker
```

If credential export fails, services remain stopped and data volumes are retained. Resolve the failure and retry `down`; do not report cleanup as complete until it succeeds.

If you have a complete validation script, use this command to manage startup and cleanup automatically:

```bash
./scripts/docker-e2e run bash /absolute/path/to/validation.sh
```

## Validation scope

This environment validates the complete local application flow on Linux, using real providers for OAuth and model calls. Finish each run within 24 hours.

Pi and Codex are available for validation; the Claude SDK host and additional ACP Agents are not yet prepared. Self-hosted edge registration, installation over the public internet, DNS, ACME, systemd, macOS, and edge deletion that requires public DNS cleanup are outside this environment's scope. Report only the clients and user flows actually validated.
