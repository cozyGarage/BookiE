from pathlib import Path
import re
import subprocess
import tempfile
import unittest


WORKFLOW = Path(__file__).resolve().parents[3] / ".github/workflows/linux-quality.yml"


class MutationWorkflowTests(unittest.TestCase):
    def test_measurements_fail_the_job_but_keep_later_measurements_running(self):
        text = WORKFLOW.read_text()
        mutation = text.split("  coverage:")[0]
        self.assertNotIn("continue-on-error:", mutation)
        self.assertEqual(mutation.count("if: ${{ !cancelled() && steps.mutation-tool.outcome == 'success' }}"), 5)
        self.assertIn("if-no-files-found: error", mutation)
        self.assertIn("branches: [linux]", mutation)
        self.assertIn("--lib --test integration value_contract -- --include-ignored", mutation)

    def test_missing_mutation_reports_cannot_pass_the_summary(self):
        text = WORKFLOW.read_text()
        section = text.split("      - name: Summarise surviving mutants\n", 1)[1]
        section = section.split("      - name: Upload mutation reports", 1)[0]
        script = section.split("        run: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines())
        script = re.sub(r"\$\{\{.*?\}\}", "success", script)
        with tempfile.TemporaryDirectory() as root:
            summary = Path(root) / "summary.md"
            def run():
                return subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], cwd=root,
                                      env={"PATH": "/usr/bin:/bin", "GITHUB_STEP_SUMMARY": str(summary)},
                                      capture_output=True, text=True)
            self.assertNotEqual(run().returncode, 0)
            self.assertIn("Measurement unavailable", summary.read_text())
            for crate in ["core", "policy", "ssh", "driver-redis", "driver-postgres"]:
                report = Path(root) / f"mutants-{crate}/mutants.out"
                report.mkdir(parents=True)
                for kind in ["caught", "missed", "timeout", "unviable"]:
                    (report / f"{kind}.txt").write_text("")
            self.assertNotEqual(run().returncode, 0)
            for report in Path(root).glob("mutants-*/mutants.out"):
                (report / "unviable.txt").write_text("mutation did not compile\n")
            self.assertNotEqual(run().returncode, 0)
            for report in Path(root).glob("mutants-*/mutants.out"):
                (report / "caught.txt").write_text("caught mutation\n")
            self.assertEqual(run().returncode, 0)
            (report / "caught.txt").unlink()
            self.assertNotEqual(run().returncode, 0)


if __name__ == "__main__":
    unittest.main()
