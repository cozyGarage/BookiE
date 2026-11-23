import importlib.util
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ReleasePublicationTests(unittest.TestCase):
    def test_nested_translation_template_is_collected_with_packages(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        block = workflow.split("      - name: Collect release files\n", 1)[1].split("      - uses:", 1)[0]
        script = "\n".join(line[10:] for line in block.splitlines()[1:] if line.startswith("          "))
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            artifacts = base / "artifacts"
            (artifacts / "translations").mkdir(parents=True)
            for name in (
                "bookie.AppImage",
                "bookie.pkg.tar.zst",
                "bookie-gpui-linux-x86_64-0.2.8.tar.gz",
                "bookie-gpui-linux-x86_64-0.2.8.tar.gz.sha256",
                "release-notes.md",
                "translations/bookie.pot",
            ):
                (artifacts / name).write_text(name)
            result = subprocess.run(["bash", "-e", "-c", script], cwd=base, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((base / "release/bookie.pot").read_text(), "translations/bookie.pot")
            for name in ("bookie-gpui-linux-x86_64-0.2.8.tar.gz", "bookie-gpui-linux-x86_64-0.2.8.tar.gz.sha256"):
                self.assertEqual((base / "release" / name).read_text(), name)
            subprocess.run(["sha256sum", "--check", "SHA256SUMS"], cwd=base / "release", check=True, capture_output=True)

    def test_gpui_preview_job_is_required_for_release_publication(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        gpui = re.search(r"(?ms)^  gpui-linux:\n(.*?)(?=^  [a-z][a-z-]*:\n|\Z)", workflow).group(1)
        publish = re.search(r"(?ms)^  publish:\n(.*?)(?=^  [a-z][a-z-]*:\n|\Z)", workflow).group(1)

        self.assertIn("name: GPUI Linux preview", gpui)
        self.assertIn("CARGO_TARGET_DIR: apps/gpui/target", gpui)
        self.assertIn("cargo build --release --locked --manifest-path apps/gpui/Cargo.toml", gpui)
        self.assertIn("install -m 0755 apps/gpui/target/release/bookie-gpui", gpui)
        self.assertIn("glibc 2.39", gpui)
        self.assertIn('name="bookie-gpui-linux-x86_64-${VERSION}"', gpui)
        self.assertIn("needs: [prepare, appimage, arch, gpui-linux]", publish)
        self.assertIn("needs.gpui-linux.result == 'success'", publish)
        self.assertIn("bookie-gpui-linux-x86_64-*.tar.gz.sha256", publish)
        self.assertIn("GTK frontend: Arch package and AppImage.", workflow)
        self.assertIn("GPUI frontend: unsupported Linux x86_64 preview tarball", workflow)

    def load_validator(self):
        spec = importlib.util.spec_from_file_location("release_recovery", ROOT / "scripts/validate-release-recovery.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def fixture(self):
        run = {"head_sha": "a" * 40, "head_branch": "linux-v0.2.0", "event": "push", "path": ".github/workflows/release.yml"}
        jobs = {"jobs": [{"name": name, "conclusion": "success"} for name in ["Arch package", "AppImage", "GPUI Linux preview"]]}
        return run, jobs

    def test_successful_packages_can_recover_a_failed_publication(self):
        run, jobs = self.fixture()
        self.load_validator().validate(run, jobs, "linux-v0.2.0", "a" * 40)

    def test_other_source_or_incomplete_packages_are_refused(self):
        validator = self.load_validator()
        for key, value in [("head_sha", "b" * 40), ("head_branch", "linux-v0.1.5"), ("event", "pull_request"), ("path", ".github/workflows/other.yml")]:
            with self.subTest(key=key):
                run, jobs = self.fixture()
                run[key] = value
                with self.assertRaises(ValueError):
                    validator.validate(run, jobs, "linux-v0.2.0", "a" * 40)
        for conclusion in ["failure", "skipped", None]:
            with self.subTest(conclusion=conclusion):
                run, jobs = self.fixture()
                jobs["jobs"][0]["conclusion"] = conclusion
                with self.assertRaises(ValueError):
                    validator.validate(run, jobs, "linux-v0.2.0", "a" * 40)
        run, jobs = self.fixture()
        jobs["jobs"].pop()
        with self.assertRaises(ValueError):
            validator.validate(run, jobs, "linux-v0.2.0", "a" * 40)

    def test_duplicate_package_jobs_are_refused(self):
        run, jobs = self.fixture()
        jobs["jobs"].append({"name": "Arch package", "conclusion": "failure"})
        with self.assertRaises(ValueError):
            self.load_validator().validate(run, jobs, "linux-v0.2.0", "a" * 40)

    def test_old_release_run_missing_gpui_reports_the_missing_job(self):
        run, jobs = self.fixture()
        jobs["jobs"].pop()
        with self.assertRaisesRegex(ValueError, "GPUI Linux preview.*missing"):
            self.load_validator().validate(run, jobs, "linux-v0.2.0", "a" * 40)

    def test_release_tag_verifier_accepts_only_a_trusted_ssh_signature(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            repository = root / "repository"
            repository.mkdir()
            signing_key = root / "signing_key"
            wrong_key = root / "wrong_key"
            allowed_signers = root / "allowed_signers"
            subprocess.run(
                ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(signing_key)],
                check=True,
                capture_output=True,
                text=True,
            )
            subprocess.run(
                ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(wrong_key)],
                check=True,
                capture_output=True,
                text=True,
            )
            public_key = signing_key.with_suffix(".pub").read_text().strip()
            allowed_signers.write_text(f"release@example.test namespaces=\"git\" {public_key}\n")
            subprocess.run(["git", "init", "--quiet", str(repository)], check=True)
            self.run_git(repository, "config", "user.name", "Release Test")
            self.run_git(repository, "config", "user.email", "release@example.test")
            self.run_git(repository, "config", "gpg.format", "ssh")
            self.run_git(repository, "config", "user.signingkey", str(signing_key))
            (repository / "source.txt").write_text("release source\n")
            self.run_git(repository, "add", "source.txt")
            self.run_git(repository, "commit", "--quiet", "-m", "release source")
            self.run_git(repository, "tag", "-s", "-m", "BookiE release", "linux-v0.3.0")

            accepted = self.verify_tag(repository, "linux-v0.3.0", allowed_signers)
            self.assertEqual(accepted.returncode, 0, accepted.stderr)

            wrong_public_key = wrong_key.with_suffix(".pub").read_text().strip()
            allowed_signers.write_text(f"release@example.test namespaces=\"git\" {wrong_public_key}\n")
            refused = self.verify_tag(repository, "linux-v0.3.0", allowed_signers)
            self.assertNotEqual(refused.returncode, 0)

            self.run_git(repository, "tag", "-a", "-m", "unsigned tag", "linux-v0.3.1")
            refused = self.verify_tag(repository, "linux-v0.3.1", allowed_signers)
            self.assertNotEqual(refused.returncode, 0)

    def run_git(self, repository: Path, *arguments: str) -> None:
        subprocess.run(["git", "-C", str(repository), *arguments], check=True, capture_output=True, text=True)

    def verify_tag(self, repository: Path, tag: str, allowed_signers: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", str(ROOT / "scripts/verify-release-tag.sh"), tag, str(allowed_signers)],
            cwd=repository,
            capture_output=True,
            text=True,
            check=False,
        )
