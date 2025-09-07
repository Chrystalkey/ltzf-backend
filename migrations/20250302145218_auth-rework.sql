-- Add migration script here
ALTER TABLE api_keys ADD COLUMN salt VARCHAR NOT NULL DEFAULT 'ltzf_defaultsalt';
ALTER TABLE api_keys ADD COLUMN keytag VARCHAR UNIQUE DEFAULT NULL;

UPDATE api_keys SET keytag = SUBSTRING(key_hash, 0, 16) WHERE keytag IS NULL;

ALTER TABLE api_keys ALTER COLUMN keytag SET NOT NULL;

ALTER TABLE api_keys DROP COLUMN coll_id;

-- replace the "deleted" column with a "deleted by" column for better bookkeeping and fault tolerance. 
-- NULL means not deleted.
-- self-reference means rotated or expired.
-- other reference means the keyadder key with that permission deleted this key deliberately.

ALTER TABLE api_keys ADD COLUMN deleted_by INTEGER REFERENCES api_keys DEFAULT NULL;

UPDATE api_keys SET deleted_by = CASE WHEN deleted IS NOT NULL THEN 1 END;

ALTER TABLE api_keys DROP COLUMN deleted;
ALTER TABLE api_keys ALTER COLUMN created_by SET NOT NULL;
ALTER TABLE api_keys ALTER COLUMN scope SET NOT NULL;