The headless MCP daemon now closes the database connection and SSH tunnel of a saved connection that an agent has not used for 15 minutes, instead of holding them until the daemon exits.
