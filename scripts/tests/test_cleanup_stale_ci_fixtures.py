import datetime
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("stale_fixtures", ROOT / "scripts/cleanup-stale-ci-fixtures.py")
cleanup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleanup)


class StaleCiFixtureCleanupTests(unittest.TestCase):
    def test_removes_only_old_ci_testcontainers_and_driver_tls_projects(self):
        now = datetime.datetime(2026, 11, 14, tzinfo=datetime.timezone.utc)
        stale = (now - datetime.timedelta(hours=4)).isoformat().replace("+00:00", "Z")
        recent = (now - datetime.timedelta(minutes=30)).isoformat().replace("+00:00", "Z")
        rows = {
            "test-old": f"test-old\t{stale}\tforgejo-10-1-driver-core\t<no value>",
            "test-recent": f"test-recent\t{recent}\tgithub-11-1-driver-analytics\t<no value>",
            "test-local": f"test-local\t{stale}\tlocal-123\t<no value>",
            "compose-old-a": f"compose-old-a\t{stale}\t<no value>\tforgejo-12-1-driver-tls",
            "compose-old-b": f"compose-old-b\t{stale}\t<no value>\tforgejo-12-1-driver-tls",
            "compose-recent": f"compose-recent\t{recent}\t<no value>\tgithub-13-1-driver-tls",
            "compose-unknown": f"compose-unknown\t{stale}\t<no value>\tother-service",
        }
        calls = []

        def fake_docker(*args):
            calls.append(args)
            if args[:3] == ("ps", "--all", "--quiet"):
                label = args[-1]
                if label == "label=com.bookie.test-run":
                    return "test-old\ntest-recent\ntest-local\n"
                return "compose-old-a\ncompose-old-b\ncompose-recent\ncompose-unknown\n"
            if args[0] == "inspect":
                return rows[args[-1]]
            if args[:2] == ("network", "ls"):
                return "network-old\n"
            if args[:2] == ("volume", "ls"):
                return "volume-old\n"
            return ""

        with patch.object(cleanup, "docker", side_effect=fake_docker):
            cleanup.reconcile(now)

        self.assertIn(("rm", "--force", "--volumes", "test-old", "compose-old-a", "compose-old-b"), calls)
        resource_calls = [call for call in calls if call[:2] in (("network", "ls"), ("volume", "ls"))]
        self.assertEqual(len(resource_calls), 2)
        self.assertTrue(all("forgejo-12-1-driver-tls" in call[-1] for call in resource_calls))
        self.assertFalse(any("github-13-1-driver-tls" in call[-1] for call in calls))
        self.assertIn(("network", "rm", "network-old"), calls)
        self.assertIn(("volume", "rm", "volume-old"), calls)


if __name__ == "__main__":
    unittest.main()
