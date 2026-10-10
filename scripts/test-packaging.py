#!/usr/bin/env python3
"""SDK discovery regression: Android platforms may have minor API versions."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('package_game', Path(__file__).with_name('package-game.py'))
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class SdkSelection(unittest.TestCase):
    def test_platform_minor_versions_previews_and_incomplete_installs(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for version in ['android-9', 'android-36', 'android-37.1', 'android-38-preview', 'android-99']:
                folder = root / version
                folder.mkdir()
                if version != 'android-99':
                    (folder / 'android.jar').touch()
            selected = package.versioned_directory(root, 'android-', ['android.jar'])
            self.assertEqual(selected.name, 'android-37.1')

    def test_build_tools_require_complete_released_installation(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            for version in ['35.0.0', '36.1.0', '37.0.0-rc1']:
                folder = root / version
                folder.mkdir()
                (folder / 'aapt').touch()
                if version != '36.1.0':
                    (folder / 'zipalign').touch()
            self.assertEqual(package.versioned_directory(root, '', ['aapt', 'zipalign']).name, '35.0.0')
            with self.assertRaises(ValueError):
                package.versioned_directory(root, '', ['missing'])


if __name__ == '__main__':
    unittest.main()
