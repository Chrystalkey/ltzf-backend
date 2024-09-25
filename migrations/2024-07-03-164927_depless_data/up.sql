-- tables that do not require any foreign keys
CREATE TABLE
    gesetzestyp (
        id SERIAL PRIMARY KEY,
        value VARCHAR(64) NOT NULL
    );
CREATE TABLE identifikatortyp (
    id SERIAL PRIMARY KEY,
    value VARCHAR(64) NOT NULL
);

CREATE TABLE
    parlament (
        id SERIAL PRIMARY KEY,
        value CHAR(2) NOT NULL
    );

CREATE TABLE
    autor (
        id SERIAL PRIMARY KEY,
        name VARCHAR(255) NOT NULL,
        organisation VARCHAR(255) NOT NULL
    );

CREATE TABLE
    schlagwort (
        id SERIAL PRIMARY KEY,
        value VARCHAR(255) NOT NULL
    );

CREATE TABLE
    dokumenttyp (id SERIAL PRIMARY KEY, value VARCHAR(255) NOT NULL);

CREATE TABLE
    stationstyp (
        id SERIAL PRIMARY KEY,
        value VARCHAR(255) NOT NULL
    );
