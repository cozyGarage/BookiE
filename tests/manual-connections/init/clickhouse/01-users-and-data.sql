CREATE USER IF NOT EXISTS bookie_ro IDENTIFIED WITH plaintext_password BY 'bookie_ro_password';
GRANT SELECT ON bookie_lab.* TO bookie_ro;

CREATE TABLE IF NOT EXISTS bookie_lab.people (
    id UInt64,
    name String,
    email Nullable(String),
    active Bool,
    created_at DateTime DEFAULT now()
) ENGINE = MergeTree
ORDER BY id;

INSERT INTO bookie_lab.people (id, name, email, active) VALUES
    (1, 'Ada Lovelace', 'ada@example.test', true),
    (2, 'Grace Hopper', 'grace@example.test', false);
