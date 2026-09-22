# Edge device connections

`pontia-edge` accepts authenticated device connections over WSS. This layer provides
device presence, heartbeats, and reconnection. Account login, device claiming, and
External API forwarding are subsequent layers.

Start edge with an existing TLS certificate and key, and a separate SQLite database:

```sh
cargo run -p pontia-edge -- \
  --bind 127.0.0.1:8443 \
  --database /absolute/path/edge.db \
  --tls-cert /absolute/path/fullchain.pem \
  --tls-key /absolute/path/key.pem
```

Enable the device connection in the device's `$PONTIA_HOME/config.toml`:

```toml
[remote]
edge_url = "wss://edge.example.com:8443/tunnel"
# Optional additional trust root for a private CA:
# ca_certificate = "/absolute/path/ca.pem"
```

Omitting `[remote]` disables remote connections. On first use, pontiad creates
`$PONTIA_HOME/state/device-identity.json` and logs the device ID and public key.
Keep this file private and persistent: it contains the device's signing key.
On Unix, a new identity file has mode `0600`.

An edge binding must already associate the device ID and public key with an account.
Until the claiming layer is available, integration-test fixtures create those records
through `DeviceBindings::bind`; there is no public enrollment endpoint. An unbound
device remains offline and retries with exponential backoff. The edge database
enforces one account per device and one device per account.

Authentication is repeated on every connection. Only successfully authenticated
connections appear online. A new authenticated connection replaces the previous
connection for the same device; missed heartbeats and disconnects remove presence.
Device presence is separate from Agent Session and Turn state.

Run the WSS integration checks with temporary certificates, keys, and databases:

```sh
SQLX_OFFLINE=true cargo test -p pontia-edge -p pontia-tunnel
```
