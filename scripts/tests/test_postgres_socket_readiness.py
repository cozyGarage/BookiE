from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/test-postgres-socket.sh"


class PostgresSocketReadinessTests(unittest.TestCase):
    def test_waits_for_the_final_server_after_database_initialization(self):
        script = SCRIPT.read_text()
        start = script.index('for _ in $(seq 1 60); do')
        end = script.index('TABLEPRO_PG_SOCKET_DIR=', start)
        readiness = script[start:end]

        init_complete = "PostgreSQL init process complete; ready for start up."
        self.assertIn(init_complete, readiness)
        probe = 'docker exec "$container_name" pg_isready -h /var/run/postgresql'
        self.assertLess(readiness.index(init_complete), readiness.index(probe))
        self.assertEqual(script.count(probe), 1)


if __name__ == "__main__":
    unittest.main()
