# Docker E2E Guide

Use the prepared local environment to validate Cloud, Public Dashboard, edge, pontiad, and real Agent calls from the current checkout. Certificates and existing test login credentials are reused automatically; routine validation requires no setup changes.

Each run starts with fresh containers and databases and automatically provisions a public edge for the test user. Remove all run data when validation ends, including Sessions, client conversations, Workspaces, and background services.

## 1. Start the environment

Run these commands from the root of the checkout you want to validate. Choose an instance name for this terminal:

```bash
export PONTIA_E2E_NAME=my-task
./scripts/docker-e2e build
./scripts/docker-e2e check
./scripts/docker-e2e up
```

The build includes uncommitted source changes and new files in the current checkout. If certificate or credential checks fail, report the specific blocker. Do not reinitialize the environment or copy authentication files from your everyday account.

Multiple instances can run on the same host. Different checkouts are isolated automatically. In the same checkout, choose a different name for each concurrent task. You can also select an instance explicitly:

```bash
./scripts/docker-e2e --name task-a up
./scripts/docker-e2e --name task-b up
./scripts/docker-e2e --name task-a login-device
./scripts/docker-e2e --name task-a down
```

Use the same name for all commands belonging to a task. `--name` takes precedence over `PONTIA_E2E_NAME`; when neither is set, the name is `default`. Build once per checkout after source changes; all instances in that checkout use those images.

If startup reports that the selected instance already exists, identify its owner before cleaning it up. Other names and other checkouts can continue running. Each instance retains its own refreshed login credentials across runs. New instances start with the prepared test credentials in `docker/e2e/.local/auth/`.

## 2. Open the container browser

Open the `Test browser` URL printed by `up` in the host browser. Each instance receives an available loopback port. To find it again:

```bash
./scripts/docker-e2e compose port browser 6080
```

Use `http://<printed-address>/vnc.html?autoconnect=1&resize=scale`.

The page provides access to the browser inside the container. Agents should use `computer-use` to operate it. For browser automation, you can also connect to CDP at `127.0.0.1:9222` inside the container through a temporary `docker exec` forwarder. Close the forwarder when finished.

Access the following URLs only in the **container browser**:

| URL | Purpose |
| --- | --- |
| `https://pontia.dev` | Cloud for this run |
| `https://app.pontia.dev` | Public Dashboard for this run |
| `http://localhost:8080/dashboard/` | Local Dashboard for this run |

Opening `pontia.dev` or `app.pontia.dev` directly on the host still reaches production. All instances use the same container URLs.

Each run creates a local Cloud test user, and the container browser starts signed in as that user at `https://pontia.dev/settings/account`.

## 3. Sign in the device

Startup provisions the public edge `docker-e2e`; no deployment ticket or manual edge registration is needed. Run:

```bash
./scripts/docker-e2e login-device
```

The command displays a device login URL and code, then waits for approval. Open the URL in the container browser, check the code, and approve it as the test user. The command then registers the device, restarts the device service, waits for it to become ready, and prints the Dashboard URL.

Open the printed Dashboard URL in the container browser. Register a new device for each run; do not reuse a device ID or Dashboard URL from a previous run. If the command fails, resolve the error before retrying.

## 4. Validate real Agent calls

Pi, Codex, and Claude Code are installed and enabled automatically. For Codex, put an OpenAI API key on a single line in `docker/e2e/.local/openai-api-key` before starting an instance. Leave the file empty to run tests that do not call Codex. To check the login method in a running instance, run `./scripts/docker-e2e compose exec -T device codex login status`.

If Pi or Claude Code needs an account login, use the device container:

```bash
./scripts/docker-e2e compose exec device bash
# Run the following commands inside the device container:
cd /home/e2e/workspace
tmux new -s e2e
# Run pi or claude auth login and complete any required authentication.
```

### Claude authentication

For OpenRouter, set `OPENROUTER_API_KEY=your-openrouter-api-key` in `docker/e2e/.local/cloud.dev.vars` before `up`. Replace the `xxx` placeholder; startup stops while it is still present. Each fresh run uses this key for Claude's OpenRouter connection. Keep this file private (`chmod 600`); no Claude subscription login is required. See [OpenRouter's Claude Code guide](https://openrouter.ai/docs/cookbook/coding-agents/claude-code-integration).

Pontia uses the container's Claude Code authentication. For account login, run `claude auth login` inside the device and open its login URL in the container browser. `down` saves the refreshed login for the next run, without retaining Claude conversations.

An API key or compatible gateway can be used without a Claude subscription. Before starting a fresh run, create `docker/e2e/.local/auth/device/claude-settings.json` with Claude's native settings format and restrict its permissions with `chmod 600`. For a direct Anthropic API key:

```json
{
  "env": {
    "ANTHROPIC_API_KEY": "YOUR_TEST_API_KEY"
  }
}
```

For a gateway, add `ANTHROPIC_BASE_URL` and use the credential variable required by that gateway: `ANTHROPIC_AUTH_TOKEN` for a bearer token or `ANTHROPIC_API_KEY` for an `x-api-key` header. Set `model` in the same settings file if the gateway requires a particular model ID. See [Claude's gateway configuration guide](https://code.claude.com/docs/en/llm-gateway-connect).

The private settings file applies to Claude Code both in the terminal and through Pontia. It is excluded from Git and image builds. Edit this host file for changes that should apply to future runs of any instance; settings and approvals changed inside a run are discarded. Do not copy your everyday Claude configuration or put real credentials in this guide, shell arguments, or validation evidence.

Check authentication with `./scripts/docker-e2e compose exec device claude auth status`. This checks the configured authentication source; only a completed model task proves the credential and provider work. Select **Claude Code** when creating the Dashboard Session, then verify file creation, tool approval, exit, history, and resume. Real model tasks use the configured provider's quota or billing.

### Complete the user flow

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

This preserves refreshed login credentials in `docker/e2e/.local/instances/<name>/auth/` and removes the selected instance's containers, networks, databases, Sessions, client conversations, Workspaces, and browser runtime data. Other instances remain running. Use the same command to clean up after a forced termination or host interruption.

Get the selected instance’s project name, then confirm that all three resource checks return no output before ending the task:

```bash
project=$(./scripts/docker-e2e compose config --format json | python3 -c 'import json,sys; print(json.load(sys.stdin)["name"])')
docker ps -aq --filter "label=com.docker.compose.project=$project"
docker volume ls -q --filter "label=com.docker.compose.project=$project"
docker network ls -q --filter "label=com.docker.compose.project=$project"
```

If credential export fails, services remain stopped and data volumes are retained. Resolve the failure and retry `down`; do not report cleanup as complete until it succeeds.

If you have a complete validation script, use this command to manage startup and cleanup automatically. Commands inside the script inherit the selected instance name:

```bash
./scripts/docker-e2e run bash /absolute/path/to/validation.sh
```

## Validation scope

This environment validates the complete local application flow on Linux, using real providers for model calls. Finish each run within 24 hours.

Pi, Codex, and Claude Code are available for validation. Claude requires a test login or API configuration. Report only the clients and user flows actually validated.
