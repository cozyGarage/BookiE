import os
from pathlib import Path
import subprocess
import unittest


class GtkWidgetScreenTests(unittest.TestCase):
    def test_screen_dimensions_preserve_logical_size_at_supported_scales(self):
        helper = Path(__file__).resolve().parents[1] / "gtk-widget-screen.sh"
        for scale, expected in (("1", "1600x1200x24"), ("2", "3200x2400x24")):
            with self.subTest(scale=scale):
                result = subprocess.run(
                    ["bash", str(helper)],
                    check=True,
                    capture_output=True,
                    env={**os.environ, "GDK_SCALE": scale},
                    text=True,
                )
                self.assertEqual(result.stdout, expected)

    def test_invalid_scale_is_rejected(self):
        helper = Path(__file__).resolve().parents[1] / "gtk-widget-screen.sh"
        result = subprocess.run(
            ["bash", str(helper)],
            capture_output=True,
            env={**os.environ, "GDK_SCALE": "1.5"},
            text=True,
        )
        self.assertEqual(result.returncode, 2)


if __name__ == "__main__":
    unittest.main()
