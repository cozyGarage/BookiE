CREATE DATABASE IF NOT EXISTS bookie_extra;
CREATE USER IF NOT EXISTS 'bookie_ro'@'%' IDENTIFIED BY 'bookie_ro_password';
GRANT SELECT ON bookie_lab.* TO 'bookie_ro'@'%';
GRANT ALL PRIVILEGES ON bookie_extra.* TO 'bookie_rw'@'%';

CREATE TABLE people (
    id BIGINT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE,
    active BOOLEAN NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO people (id, name, email, active) VALUES
    (1, 'Ada Lovelace', 'ada@example.test', true),
    (2, 'Grace Hopper', 'grace@example.test', false);

GRANT SELECT, INSERT, UPDATE, DELETE ON bookie_lab.* TO 'bookie_rw'@'%';
FLUSH PRIVILEGES;
