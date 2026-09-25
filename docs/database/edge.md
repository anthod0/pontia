# Edge SQLite database

### `devices`

| Column | Type | Constraints / default |
|---|---|---|
| `device_id` | TEXT | primary key, NOT NULL |
| `public_key` | BLOB | NOT NULL, UNIQUE |
| `created_at` | TEXT | NOT NULL, default `strftime('%Y-%m-%dT%H:%M:%fZ', 'now')` |
