IF NOT EXISTS (SELECT 1 FROM sys.server_principals WHERE name = 'bookie')
BEGIN
    CREATE LOGIN bookie WITH PASSWORD = 'bookie', CHECK_POLICY = OFF;
END;

IF NOT EXISTS (SELECT 1 FROM sys.databases WHERE name = 'bookie')
BEGIN
    CREATE DATABASE bookie;
END;

ALTER SERVER ROLE sysadmin ADD MEMBER bookie;
