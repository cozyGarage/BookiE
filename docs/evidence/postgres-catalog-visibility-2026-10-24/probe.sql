CREATE ROLE catalog_limited;
CREATE SCHEMA bookie_visible;
CREATE SCHEMA bookie_hidden;
CREATE TABLE bookie_visible.allowed_rows(id integer);
CREATE TABLE bookie_hidden.denied_rows(id integer);
CREATE FUNCTION bookie_hidden.hidden_function() RETURNS integer LANGUAGE sql AS 'SELECT 1';
GRANT USAGE ON SCHEMA bookie_visible TO catalog_limited;
GRANT SELECT ON bookie_visible.allowed_rows TO catalog_limited;
SET ROLE catalog_limited;
SELECT 'pg_tables=' || string_agg(schemaname || '.' || tablename, ',' ORDER BY schemaname, tablename)
FROM pg_catalog.pg_tables
WHERE schemaname LIKE 'bookie_%';
SELECT 'information_schema=' || string_agg(table_schema || '.' || table_name, ',' ORDER BY table_schema, table_name)
FROM information_schema.tables
WHERE table_schema LIKE 'bookie_%';
SELECT 'privileges=' || coalesce((SELECT has_schema_privilege(n.oid, 'USAGE')::text FROM pg_catalog.pg_namespace n WHERE n.nspname = 'bookie_hidden'), 'missing')
    || ',' || coalesce((SELECT has_table_privilege(c.oid, 'SELECT')::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'bookie_hidden' AND c.relname = 'denied_rows'), 'missing')
    || ',' || (SELECT has_schema_privilege(n.oid, 'USAGE')::text FROM pg_catalog.pg_namespace n WHERE n.nspname = 'bookie_visible')
    || ',' || (SELECT has_table_privilege(c.oid, 'SELECT')::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = 'bookie_visible' AND c.relname = 'allowed_rows');
SELECT 'pg_proc=' || string_agg(n.nspname || '.' || p.proname, ',')
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'bookie_hidden' AND p.proname = 'hidden_function';
