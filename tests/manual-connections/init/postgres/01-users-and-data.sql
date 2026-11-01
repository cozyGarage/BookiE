CREATE ROLE bookie_rw LOGIN PASSWORD 'bookie_rw_password';
CREATE ROLE bookie_ro LOGIN PASSWORD 'bookie_ro_password';
CREATE DATABASE bookie_extra OWNER bookie_rw;

CREATE TABLE people (
    id BIGINT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    active BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO people (id, name, email, active) VALUES
    (1, 'Ada Lovelace', 'ada@example.test', true),
    (2, 'Grace Hopper', 'grace@example.test', false);

GRANT CONNECT ON DATABASE bookie_lab TO bookie_rw, bookie_ro;
GRANT USAGE ON SCHEMA public TO bookie_rw, bookie_ro;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO bookie_rw;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO bookie_ro;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO bookie_rw;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO bookie_ro;
