#!/usr/bin/env python3
"""Fail-closed tests for the historical checksum drift policy."""
import hashlib
import tempfile
import unittest
from pathlib import Path

from audit_closeout import read_drift_ledger, verify_evidence_checksums


class HistoricalDriftTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.source = self.root / 'source.rs'
        self.source.write_text('original')
        self.original = hashlib.sha256(b'original').hexdigest()
        self.sums = self.root / 'SHA256SUMS'
        self.sums.write_text(f'{self.original}  source.rs\n')

    def tearDown(self):
        self.temp.cleanup()

    def test_only_exact_reviewed_drift_passes(self):
        self.source.write_text('updated')
        current = hashlib.sha256(b'updated').hexdigest()
        record = ('source.rs', self.original, current)
        errors, used = verify_evidence_checksums(self.root, [self.sums], {record})
        self.assertFalse(errors)
        self.assertEqual(used, {record})
        self.source.write_text('tampered')
        errors, _ = verify_evidence_checksums(self.root, [self.sums], {record})
        self.assertTrue(errors)

    def test_unrecorded_and_stale_changes_fail(self):
        self.source.write_text('updated')
        errors, _ = verify_evidence_checksums(self.root, [self.sums], set())
        self.assertTrue(errors)
        self.source.write_text('original')
        record = ('source.rs', self.original, hashlib.sha256(b'updated').hexdigest())
        errors, _ = verify_evidence_checksums(self.root, [self.sums], {record})
        self.assertTrue(errors)

    def test_evidence_itself_can_never_be_exempted(self):
        evidence = self.root / 'docs/evidence/c0/nc.log'
        evidence.parent.mkdir(parents=True)
        evidence.write_text('altered evidence')
        original = hashlib.sha256(b'original evidence').hexdigest()
        current = hashlib.sha256(b'altered evidence').hexdigest()
        self.sums.write_text(f'{original}  docs/evidence/c0/nc.log\n')
        errors, _ = verify_evidence_checksums(self.root, [self.sums], {('docs/evidence/c0/nc.log', original, current)})
        self.assertTrue(errors)

    def test_malformed_and_unsafe_ledger_fails(self):
        ledger = self.root / 'ledger'
        ledger.write_text('not a valid line\n')
        with self.assertRaises(ValueError):
            read_drift_ledger(ledger)
        ledger.write_text(f'{self.original} {"a" * 64} ../outside\n')
        with self.assertRaises(ValueError):
            read_drift_ledger(ledger)
        ledger.write_text(f'{self.original} {"a" * 64} docs/evidence/c0/nc.log\n')
        with self.assertRaises(ValueError):
            read_drift_ledger(ledger)


if __name__ == '__main__':
    unittest.main()
