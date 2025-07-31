-- alterations to the earlier database state

ALTER TABLE scraper_touched_vorgang ADD CONSTRAINT "unique_scraper-vg" UNIQUE (vg_id, scraper);
ALTER TABLE scraper_touched_vorgang ADD COLUMN collector_key INTEGER REFERENCES api_keys(id) NOT NULL;
ALTER TABLE scraper_touched_station ADD CONSTRAINT "unique_scraper-sn" UNIQUE (stat_id, scraper);
ALTER TABLE scraper_touched_station ADD COLUMN collector_key INTEGER REFERENCES api_keys(id) NOT NULL;
ALTER TABLE scraper_touched_dokument ADD CONSTRAINT "unique_scraper-dk" UNIQUE (dok_id, scraper);
ALTER TABLE scraper_touched_dokument ADD COLUMN collector_key INTEGER REFERENCES api_keys(id) NOT NULL;
ALTER TABLE scraper_touched_sitzung ADD CONSTRAINT "unique_scraper-sg" UNIQUE (sid, scraper);
ALTER TABLE scraper_touched_sitzung ADD COLUMN collector_key INTEGER REFERENCES api_keys(id) NOT NULL;
ALTER TABLE api_keys ADD COLUMN rotated_for INTEGER REFERENCES api_keys(id) DEFAULT NULL;

ALTER TABLE vorgangstyp ADD UNIQUE(value);
ALTER TABLE vg_ident_typ ADD UNIQUE(value);
ALTER TABLE parlament ADD UNIQUE(value);
ALTER TABLE dokumententyp ADD UNIQUE(value);
ALTER TABLE schlagwort ADD UNIQUE(value);
ALTER TABLE stationstyp ADD UNIQUE(value);

-- make gr_id NOT NULL and remove p_id column
ALTER TABLE station DROP COLUMN p_id;
ALTER TABLE station DROP CONSTRAINT station_gr_id_fkey;
UPDATE station SET gr_id = 0 WHERE gr_id IS NULL;
ALTER TABLE station ALTER COLUMN gr_id SET NOT NULL;
ALTER TABLE station ADD CONSTRAINT station_gr_id_fkey FOREIGN KEY (gr_id) REFERENCES gremium(id) ON DELETE CASCADE;
