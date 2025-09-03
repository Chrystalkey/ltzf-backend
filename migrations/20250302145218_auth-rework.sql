-- Add migration script here
ALTER TABLE api_keys ADD COLUMN salt VARCHAR NOT NULL DEFAULT 'ltzf_defaultsalt';
ALTER TABLE api_keys ADD COLUMN keytag VARCHAR UNIQUE DEFAULT NULL;

UPDATE api_keys SET keytag = SUBSTRING(key_hash, 0, 16) WHERE keytag IS NULL;

ALTER TABLE api_keys ALTER COLUMN keytag SET NOT NULL;

ALTER TABLE api_keys DROP COLUMN coll_id;