from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[2]
COMPOSE = ROOT / "tests/fixtures/driver-tls/docker-compose.yml"


class DriverTlsFixtureTests(unittest.TestCase):
    def test_every_mongo_service_restarts_when_mongod_exits_during_initialisation(self):
        # The mongo image runs a temporary mongod to apply /docker-entrypoint-initdb.d
        # seeds. When the final mongod binds 27017 before that instance has released it,
        # it exits 48 (address in use) before any test runs (#924). Docker restarts the
        # container, so the second start binds cleanly.
        services = yaml.safe_load(COMPOSE.read_text())["services"]
        mongo = {name: svc for name, svc in services.items() if "mongo" in name}
        self.assertEqual(sorted(mongo), ["mongo", "mongo-plain"])
        for name, service in mongo.items():
            with self.subTest(service=name):
                self.assertEqual(service.get("restart"), "on-failure:3")


if __name__ == "__main__":
    unittest.main()
