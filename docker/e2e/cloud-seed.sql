-- Local reference vocabulary only. Users, tickets, edges and devices are
-- created through the normal authenticated application flows.
INSERT INTO hero_name_modifiers (word) VALUES ('brave'), ('calm'), ('silent')
ON CONFLICT (word) DO NOTHING;
INSERT INTO hero_name_heroes (word) VALUES ('arthur'), ('atlas'), ('orion')
ON CONFLICT (word) DO NOTHING;
