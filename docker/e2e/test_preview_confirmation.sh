#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

edge_state() {
  scripts/docker-e2e compose exec -T cloud ./node_modules/.bin/wrangler d1 execute DB \
    --local --persist-to /state --json --command "$1" | jq -e '.[0].results'
}

for scope in public private; do
  edge_state "UPDATE edges SET access_scope='$scope', preview_enabled=0
              WHERE tunnel_url='wss://docker-e2e.edge.pontia.dev/tunnel' RETURNING preview_enabled" |
    jq -e '. == [{"preview_enabled": 0}]'

  scripts/docker-e2e compose exec -T edge sh -ec '
    read -r credential < /etc/pontia/edge/credential
    printf "Authorization: Bearer %s\n" "$credential" |
      curl --fail-with-body --silent --show-error --header @- \
        --request POST https://pontia.dev/api/edge/preview/confirm
  ' | jq -e '.preview_base_hostname == "docker-e2e.pontia.run" and .manage_certificate == true'

  edge_state "SELECT preview_enabled FROM edges
              WHERE tunnel_url='wss://docker-e2e.edge.pontia.dev/tunnel'" |
    jq -e '. == [{"preview_enabled": 1}]'
done
