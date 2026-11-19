A failed transaction batch on a MySQL or MariaDB connection now closes the connection it ran on, so session effects from the failed batch cannot reach later queries.
