import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import case_oracle


class CaseBoundary(unittest.TestCase):
    def test_unexpected_pass_and_wrong_failure_are_rejected(self):
        for result in [
            SimpleNamespace(returncode=0, stdout='19\n', stderr=''),
            SimpleNamespace(returncode=1, stdout='', stderr='TypeError: failed\n'),
            SimpleNamespace(returncode=1, stdout='', stderr="SyntaxError: Unexpected token 'return'\n"),
        ]:
            with patch.object(case_oracle.subprocess, 'run', return_value=result), patch.object(Path, 'write_text'):
                with self.assertRaisesRegex(ValueError, 'changed or unexpectedly passed'):
                    case_oracle.compare_boundary()
