"""Contrat exécutable du filtre CI RustSec, sur données synthétiques uniquement."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest


FILTER = Path(__file__).with_name("check_audit_warnings.sh")


class AuditWarningsTest(unittest.TestCase):
    def run_filter(self, warnings, packages):
        with tempfile.TemporaryDirectory(prefix="audit warnings ") as folder:
            output = Path(folder)
            report = output / "audit.json"
            graph = output / "macos-packages.log"
            report.write_text(json.dumps({"warnings": warnings}))
            graph.write_text(packages)
            result = subprocess.run(
                ["bash", str(FILTER), str(report), str(graph), str(output)],
                capture_output=True, text=True, check=False,
            )
            return result, (output / "audit-macos-warnings.log").read_text()

    def test_no_warnings_pass(self):
        result, blocked = self.run_filter({}, "copyq-rs v0.3.2\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(blocked, "")

    def test_warning_outside_macos_passes(self):
        result, blocked = self.run_filter({"unsound": [{
            "kind": "unsound", "package": {"name": "glib", "version": "0.18.5"},
            "advisory": {"id": "RUSTSEC-2024-0429"},
        }]}, "copyq-rs v0.3.2\nrustix v1.1.6\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(blocked, "")

    def test_matching_macos_name_and_version_fails(self):
        result, blocked = self.run_filter({"unsound": [{
            "kind": "unsound", "package": {"name": "rustix", "version": "1.1.5"},
            "advisory": {"id": "RUSTSEC-2026-0486"},
        }]}, "copyq-rs v0.3.2\nrustix v1.1.5\n")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(blocked, "rustix v1.1.5 unsound RUSTSEC-2026-0486\n")
        self.assertIn("::error::", result.stdout)

    def test_other_version_of_same_package_passes(self):
        result, blocked = self.run_filter({"unsound": [{
            "kind": "unsound", "package": {"name": "rustix", "version": "1.1.5"},
            "advisory": {"id": "RUSTSEC-2026-0486"},
        }]}, "copyq-rs v0.3.2\nrustix v0.38.44\nrustix v1.1.6\n")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(blocked, "")

    def test_yanked_macos_package_without_advisory_fails(self):
        result, blocked = self.run_filter({"yanked": [{
            "kind": "yanked", "package": {"name": "tokio", "version": "1.54.0"},
        }]}, "copyq-rs v0.3.2\ntokio v1.54.0\n")
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(blocked, "tokio v1.54.0 yanked -\n")


if __name__ == "__main__":
    unittest.main()
