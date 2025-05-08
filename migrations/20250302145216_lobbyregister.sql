CREATE TABLE lobbyregistereintrag(
    id SERIAL PRIMARY KEY,
    vg_id INTEGER NOT NULL REFERENCES vorgang(id) ON DELETE CASCADE,
    organisation INTEGER NOT NULL REFERENCES autor(id) ON DELETE CASCADE,
    interne_id VARCHAR NOT NULL,
    intention VARCHAR NOT NULL,
    link VARCHAR NOT NULL
);

CREATE TABLE rel_lobbyreg_drucksnr(
    lob_id INTEGER NOT NULL REFERENCES lobbyregistereintrag(id) ON DELETE CASCADE,
    drucksnr VARCHAR NOT NULL,
    PRIMARY KEY (lob_id, drucksnr)
);