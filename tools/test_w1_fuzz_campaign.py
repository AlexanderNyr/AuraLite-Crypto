#!/usr/bin/env python3
"""Fail-closed unit tests for the W1 local campaign runner and v2 evidence verifier."""
from contextlib import redirect_stdout
from io import StringIO
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

import verify_w1_fuzz_evidence as verifier
import w1_fuzz_campaign as runner


class CampaignTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="w1-campaign-test-")
        self.base = Path(self.tmp.name)
        self.repo = self.base / "repo"
        self.repo.mkdir()
        for rel in (
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "tools/w1_fuzz_campaign.py",
            "tools/verify_w1_fuzz_evidence.py",
            "crates/sample.rs",
            "third_party/sample.rs",
        ):
            path = self.repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"// {rel}\n", encoding="utf-8")
        for target, _, _ in runner.TARGETS:
            corpus = self.repo / "fuzz" / "corpus" / target
            corpus.mkdir(parents=True)
            (corpus / "seed").write_bytes(b"public seed")
        self.out = self.base / "output"
        self.root_patch = mock.patch.object(runner, "ROOT", self.repo)
        self.root_patch.start()
        self.addCleanup(self.root_patch.stop)
        self.addCleanup(self.tmp.cleanup)

    def invoke(self, fake, manifests=None):
        hashes = manifests or [runner.source_manifest(self.repo)]
        sources = mock.patch.object(
            runner,
            "source_manifest",
            side_effect=iter(hashes) if len(hashes) > 1 else None,
            return_value=hashes[0],
        )
        with (
            sources,
            mock.patch.object(runner, "version", return_value="public test version"),
            mock.patch.object(runner.subprocess, "run", side_effect=fake),
            redirect_stdout(StringIO()),
        ):
            return runner.run(self.out, 10, 123)

    @staticmethod
    def good(command, **kwargs):
        kwargs["stdout"].write("Done 42 runs in 11 second(s)\n")
        return subprocess.CompletedProcess(command, 0)

    def test_completion_records_exact_four_targets_and_seeds(self):
        self.assertEqual(self.invoke(self.good), 0)
        data = json.loads((self.out / "result.json").read_text())
        self.assertEqual(data["status"], "completed")
        self.assertEqual([x["seed"] for x in data["targets"]], [123, 124, 125, 126])
        self.assertEqual([x["weak_test_only_kdf"] for x in data["targets"]], [False, True, False, False])
        self.assertTrue(all(x["fuzzer_done"] == [42, 11] for x in data["targets"]))
        self.assertTrue(all((self.out / x["target"] / "initial-corpus" / "seed").is_file() for x in data["targets"]))

    def test_early_success_is_not_completion(self):
        def early(command, **kwargs):
            kwargs["stdout"].write("Done 42 runs in 2 second(s)\n")
            return subprocess.CompletedProcess(command, 0)

        self.assertEqual(self.invoke(early), 1)
        self.assertEqual(json.loads((self.out / "result.json").read_text())["status"], "failed_or_incomplete")

    def test_zero_exit_without_done_and_crash_are_not_completion(self):
        for code, text in ((0, "build passed\n"), (1, "Done 42 runs in 11 second(s)\n")):
            with self.subTest(code=code), tempfile.TemporaryDirectory(dir=self.base) as temp:
                self.out = Path(temp) / "output"

                def fake(command, **kwargs):
                    kwargs["stdout"].write(text)
                    return subprocess.CompletedProcess(command, code)

                self.assertEqual(self.invoke(fake), 1)

    def test_source_drift_fails_closed(self):
        base_manifest = runner.source_manifest(self.repo)
        drifted = dict(base_manifest, **{"crates/sample.rs": hashlib.sha256(b"changed").hexdigest()})
        self.assertEqual(self.invoke(self.good, [base_manifest, base_manifest, drifted]), 1)
        self.assertFalse(json.loads((self.out / "result.json").read_text())["targets"][0]["source_unchanged"])

    def test_artifact_or_broken_symlink_fails_closed_even_with_zero_exit(self):
        def fake_file(command, **kwargs):
            prefix = next(arg.split("=", 1)[1] for arg in command if arg.startswith("-artifact_prefix="))
            (Path(prefix) / "crash-public").write_bytes(b"test")
            return self.good(command, **kwargs)

        self.assertEqual(self.invoke(fake_file), 1)
        self.assertEqual(
            json.loads((self.out / "result.json").read_text())["targets"][0]["artifacts"],
            ["artifacts/crash-public"],
        )
        with tempfile.TemporaryDirectory(dir=self.base) as temp:
            self.out = Path(temp) / "output"

            def fake_symlink(command, **kwargs):
                prefix = next(arg.split("=", 1)[1] for arg in command if arg.startswith("-artifact_prefix="))
                (Path(prefix) / "dangling-link").symlink_to(Path(prefix) / "nonexistent")
                return self.good(command, **kwargs)

            self.assertEqual(self.invoke(fake_symlink), 1)
            self.assertEqual(
                json.loads((self.out / "result.json").read_text())["targets"][0]["artifacts"],
                ["artifacts/dangling-link"],
            )

    def test_interrupt_never_marks_success(self):
        def interrupt(*args, **kwargs):
            raise KeyboardInterrupt()

        with self.assertRaises(KeyboardInterrupt):
            self.invoke(interrupt)
        self.assertEqual(json.loads((self.out / "result.json").read_text())["status"], "running")

    def test_existing_and_inside_checkout_are_refused(self):
        self.out.mkdir()
        (self.out / "keep").write_text("untouched")
        with self.assertRaises(FileExistsError):
            runner.run(self.out, 10, 123)
        self.assertEqual((self.out / "keep").read_text(), "untouched")
        with self.assertRaises(ValueError):
            runner.run(self.repo / "output", 10, 123)

    def test_verifier_accepts_intact_and_checkout_match_and_rejects_tampering(self):
        self.assertEqual(self.invoke(self.good), 0)
        verifier.verify(self.out)
        verifier.verify(self.out, checkout=self.repo)
        (self.repo / "crates/sample.rs").write_text("// modified after run\n", encoding="utf-8")
        with self.assertRaises(ValueError):
            verifier.verify(self.out, checkout=self.repo)
        (self.repo / "crates/sample.rs").write_text("// crates/sample.rs\n", encoding="utf-8")
        verifier.verify(self.out, checkout=self.repo)

        first = self.out / runner.TARGETS[0][0]
        cases = [
            first / "initial-corpus" / "seed",
            first / "corpus" / "seed",
            first / "run.log",
            self.out / "source-sha256.json",
            first / "corpus-sha256.json",
        ]
        for path in cases:
            with self.subTest(path=path), path.open("rb") as original:
                data = original.read()
                path.write_bytes(data + b"tampered")
                with self.assertRaises((ValueError, json.JSONDecodeError)):
                    verifier.verify(self.out)
                path.write_bytes(data)
        extra = first / "artifacts" / "unexpected"
        extra.write_bytes(b"artifact")
        with self.assertRaises(ValueError):
            verifier.verify(self.out)
        extra.unlink()
        verifier.verify(self.out)

    def test_verifier_rejects_incomplete_wrong_command_booleans_and_nonstandard_json(self):
        self.assertEqual(self.invoke(self.good), 0)
        path = self.out / "result.json"
        baseline = path.read_text(encoding="utf-8")

        for mutator in (
            lambda d: d["targets"][0]["command"].__setitem__(-2, "-seed=999"),
            lambda d: d.__setitem__("status", "running"),
            lambda d: d["targets"][0].__setitem__("exit_code", False),
            lambda d: d["targets"][0].__setitem__("fuzzer_done", [True, 11]),
        ):
            data = json.loads(baseline)
            mutator(data)
            path.write_text(json.dumps(data), encoding="utf-8")
            with self.assertRaises(ValueError):
                verifier.verify(self.out)

        path.write_text('{"format_version":2,"format_version":2}', encoding="utf-8")
        with self.assertRaises(ValueError):
            verifier.verify(self.out)
        path.write_text('{"format_version":NaN}', encoding="utf-8")
        with self.assertRaises(ValueError):
            verifier.verify(self.out)
        path.write_text(baseline, encoding="utf-8")
        verifier.verify(self.out)

    def test_corpus_drift_and_symlink_fail_closed(self):
        def mutate(command, **kwargs):
            corpus = Path(next(x for x in command if x.endswith("/corpus")))
            (corpus.parent / "initial-corpus" / "seed").write_bytes(b"modified")
            return self.good(command, **kwargs)

        self.assertEqual(self.invoke(mutate), 1)
        self.assertFalse(json.loads((self.out / "result.json").read_text())["targets"][0]["initial_corpus_unchanged"])
        with self.assertRaises(ValueError):
            verifier.verify(self.out)
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            (directory / "source").write_bytes(b"public")
            (directory / "link").symlink_to(directory / "source")
            with self.assertRaises(ValueError):
                runner.tree_manifest(directory)

    def test_verifier_rejects_older_format(self):
        self.out.mkdir()
        (self.out / "result.json").write_text('{"status":"completed"}')
        with self.assertRaises(ValueError):
            verifier.verify(self.out)

    def test_parse_done_rejects_partial_duplicate_or_zero_runs(self):
        for text in (
            "",
            "Done 0 runs in 10 second(s)",
            "Done 4 runs in",
            "Done 4 runs in 10 seconds",
            "Done 4 runs in 10 second(s)\nDone 5 runs in 10 second(s)\n",
        ):
            self.assertIsNone(runner.parse_done(text))
        self.assertEqual(runner.parse_done("noise\nDone 42 runs in 10 second(s)\n"), (42, 10))


if __name__ == "__main__":
    unittest.main()
