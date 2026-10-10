#!/usr/bin/env python3
"""Exercise source-edit persistence, conflicts, retries, rollback and failed checks."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('code_edit', Path(__file__).with_name('code-edit.py'))
edit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(edit)


class Transactions(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / 'examples/rust_game/src/main.rs'
        self.source.parent.mkdir(parents=True)
        self.source.write_text('fn main() {}\n')
        self.base = self.source.read_text()
        self.after = 'fn main() { println!("Game"); }\n'
        self.request = dict(schema_version=1, request_id='edit_1', path=str(self.source.relative_to(self.root)), expected_sha256=edit.digest(self.base), content=self.after)

    def run_edit(self, request=None, *args):
        if request is not None:
            path = self.root / 'request.json'
            path.write_text(json.dumps(request))
            argv = ['code-edit', str(path), *args]
        else:
            argv = ['code-edit', *args]
        output = io.StringIO()
        with patch.object(edit, 'ROOT', self.root), patch.object(sys, 'argv', argv), contextlib.redirect_stdout(output):
            edit.main()
        return json.loads(output.getvalue())

    def test_edit_retry_rollback_and_monotonic_conflict(self):
        self.assertFalse(self.run_edit(self.request)['replayed'])
        self.assertEqual(self.source.read_text(), self.after)
        self.assertTrue(self.run_edit(self.request)['replayed'])
        with self.assertRaises(ValueError):
            self.run_edit(dict(self.request, content='different'))
        self.run_edit(None, '--rollback', 'edit_1', '--request-id', 'undo_1')
        self.assertEqual(self.source.read_text(), self.base)
        self.assertTrue(self.run_edit(None, '--rollback', 'edit_1', '--request-id', 'undo_1')['replayed'])
        self.source.write_text('someone else changed it\n')
        with self.assertRaises(ValueError):
            self.run_edit(dict(self.request, request_id='edit_2'))
        self.assertEqual(self.source.read_text(), 'someone else changed it\n')

    def test_failed_check_and_interrupted_write_preserve_source(self):
        with patch.object(edit, 'checked', side_effect=ValueError('compiler rejected candidate')):
            with self.assertRaises(ValueError):
                self.run_edit(self.request, '--check')
        self.assertEqual(self.source.read_text(), self.base)
        self.assertFalse((self.root / '.tomo-code-journal/edit_1.json').exists())
        temp = self.source.with_name(self.source.name + '.tomo-tmp')
        temp.write_text('external recovery data')
        with self.assertRaises(FileExistsError):
            self.run_edit(self.request)
        self.assertEqual(self.source.read_text(), self.base)
        self.assertEqual(temp.read_text(), 'external recovery data')
        temp.unlink()
        self.run_edit(self.request)
        self.assertEqual(self.source.read_text(), self.after)

    def test_rejects_unknown_fields_and_paths_outside_sdk(self):
        for request in [dict(self.request, schema_version=True), dict(self.request, extra=True), dict(self.request, path='../outside.rs'), dict(self.request, expected_sha256='bad'), dict(self.request, content='a' * 262145)]:
            with self.assertRaises(ValueError):
                self.run_edit(request)
        self.assertEqual(self.source.read_text(), self.base)


if __name__ == '__main__':
    unittest.main()
