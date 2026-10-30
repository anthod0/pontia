-- The public edge owner has no login account; normal OAuth users can select this edge.
INSERT INTO users (id, display_name)
VALUES ('{owner_id}', 'E2E edge operator')
ON CONFLICT (id) DO NOTHING;

INSERT INTO edges (id, user_id, access_scope, name, dns_label, tunnel_url, service_credential_hash)
VALUES ('{edge_id}', '{owner_id}', 'public', '{name}', '{name}',
        'wss://{hostname}/tunnel', '{credential_hash}')
ON CONFLICT (id) DO NOTHING;
