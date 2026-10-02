import importlib.util
import json
import os
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest.mock import patch

REPO_ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("cargo_setup", REPO_ROOT / "scripts/cargo-setup.py")
setup = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(setup)


class CargoSetupTests(unittest.TestCase):
    def setUp(self):
        scratch = REPO_ROOT / "dist/tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.temp = tempfile.TemporaryDirectory(dir=scratch)
        self.addCleanup(self.temp.cleanup)
        self.main = Path(self.temp.name) / "main checkout"
        self.main.mkdir()
        self.git("init", str(self.main))
        (self.main / "Cargo.toml").write_text(
            '[workspace]\n[package]\nname = "setup-fixture"\nversion = "0.1.0"\nedition = "2021"\n'
        )
        (self.main / "src").mkdir()
        (self.main / "src/lib.rs").write_text("")
        self.git("-C", str(self.main), "add", ".")
        self.git("-C", str(self.main), "-c", "user.name=Test", "-c", "user.email=test@example.com",
                 "-c", "core.hooksPath=/dev/null", "commit", "-m", "fixture")
        self.worktree = Path(self.temp.name) / "external worktree"
        self.git("-C", str(self.main), "worktree", "add", "--detach", str(self.worktree))
        self.tool = setup.tool_path(self.main)
        self.tool.parent.mkdir(parents=True)
        self.tool.write_text("fixture executable")
        self.tool.chmod(0o755)

    def git(self, *args):
        return subprocess.check_output(["git", *args], stderr=subprocess.PIPE, text=True)

    def read_config(self, checkout):
        return tomllib.loads((checkout / ".cargo/config.toml").read_text(encoding="utf-8"))

    def test_primary_and_external_worktree_use_same_absolute_target_and_tool(self):
        for checkout in [self.main, self.worktree]:
            with self.subTest(checkout=checkout):
                old_target = checkout / "target/existing-artifact"
                old_target.parent.mkdir()
                old_target.write_text("keep")
                setup.configure(checkout)
                self.assertEqual(old_target.read_text(), "keep")
                config = self.read_config(checkout)
                self.assertEqual(config["build"]["target-dir"], str(self.main / "target"))
                self.assertEqual(config["build"]["rustc-wrapper"], str(self.tool))
                self.assertFalse(config["build"]["incremental"])
                self.assertEqual(config["build"]["rustc-workspace-wrapper"], "")
                self.assertEqual(config["env"]["SCCACHE_DIR"]["value"], str(self.main / "dist/cache/sccache"))
                self.assertEqual(config["env"]["SCCACHE_CACHE_SIZE"]["value"], "5G")
                setup.check(checkout, {})
                self.assertEqual(self.git("-C", str(checkout), "check-ignore", ".cargo/config.toml").strip(),
                                 ".cargo/config.toml")
        setup.configure(self.worktree)
        self.assertEqual(self.read_config(self.main), self.read_config(self.worktree))

    def test_configuration_preserves_unicode_paths(self):
        main = self.main / "unicode 🎚"
        config = tomllib.loads(setup.config_text(main))
        self.assertEqual(config["build"]["target-dir"], str(main / "target"))

    def test_refuses_to_replace_existing_cargo_configuration(self):
        for filename in ["config.toml", "config"]:
            with self.subTest(filename=filename):
                directory = self.worktree / ".cargo"
                directory.mkdir(exist_ok=True)
                path = directory / filename
                original = '[build]\njobs = 2\n'
                path.write_text(original)
                with self.assertRaisesRegex(RuntimeError, "existing Cargo config"):
                    setup.configure(self.worktree)
                self.assertEqual(path.read_text(), original)
                path.unlink()

    def test_check_detects_missing_config_tool_and_conflicting_overrides(self):
        with self.assertRaisesRegex(RuntimeError, "cargo-setup"):
            setup.check(self.worktree, {})
        setup.configure(self.worktree)
        for env in [
            {"CARGO_TARGET_DIR": str(self.worktree / "target")},
            {"CARGO_BUILD_TARGET_DIR": "other"},
            {"RUSTC_WRAPPER": ""},
            {"CARGO_BUILD_RUSTC_WRAPPER": "other"},
            {"CARGO_INCREMENTAL": "1"},
            {"CARGO_BUILD_INCREMENTAL": "true"},
            {"RUSTC_WORKSPACE_WRAPPER": "other"},
            {"SCCACHE_SERVER_UDS": "/tmp/other-repo.sock"},
            {"SCCACHE_SERVER_UDS": ""},
        ]:
            with self.subTest(env=env), self.assertRaisesRegex(RuntimeError, "override"):
                setup.check(self.worktree, env)
        setup.check(self.worktree, {"CARGO_TARGET_DIR": str(self.main / "target"), "CARGO_INCREMENTAL": "0"})
        self.tool.unlink()
        with self.assertRaisesRegex(RuntimeError, "cargo-setup"):
            setup.check(self.worktree, {})

    def test_check_rejects_incremental_flags_but_allows_unrelated_rustflags(self):
        setup.configure(self.worktree)
        for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "CARGO_BUILD_RUSTFLAGS",
                    "CARGO_TARGET_AARCH64_APPLE_DARWIN_RUSTFLAGS"]:
            for flags in ["-C incremental=/tmp/other", "-Cincremental=/tmp/other",
                          "--codegen incremental=/tmp/other", "--codegen=incremental=/tmp/other",
                          "-C=incremental=/tmp/other"]:
                value = flags.replace(" ", "\x1f") if key == "CARGO_ENCODED_RUSTFLAGS" else flags
                with self.subTest(key=key, flags=flags), self.assertRaisesRegex(RuntimeError, "override"):
                    setup.check(self.worktree, {key: value})
            setup.check(self.worktree, {key: "-Copt-level=1"})

    def test_local_config_clears_inherited_workspace_wrapper(self):
        parent_config = Path(self.temp.name) / ".cargo/config.toml"
        parent_config.parent.mkdir()
        parent_config.write_text('[build]\nrustc-workspace-wrapper = "missing-workspace-wrapper"\n')
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("CARGO_") and key not in ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"]}
        env.update(RUSTC_WRAPPER="", CARGO_TARGET_DIR=str(self.main / "target"))
        command = ["cargo", "check", "--offline"]
        before = subprocess.run(command, cwd=self.worktree, env=env, capture_output=True, text=True)
        self.assertNotEqual(before.returncode, 0)
        self.assertIn("missing-workspace-wrapper", before.stderr)
        setup.configure(self.worktree)
        result = subprocess.run(command, cwd=self.worktree, env=env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.read_config(self.worktree)["build"]["rustc-workspace-wrapper"], "")

    def test_check_rejects_modified_generated_configuration(self):
        setup.configure(self.worktree)
        path = self.worktree / ".cargo/config.toml"
        path.write_text(path.read_text().replace('incremental = false', 'incremental = true'))
        with self.assertRaisesRegex(RuntimeError, "cargo-setup"):
            setup.check(self.worktree, {})

    def test_cargo_metadata_resolves_shared_target_from_root_and_nested_crate(self):
        setup.configure(self.worktree)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("CARGO_") and key not in ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"]}
        env["RUSTC_WRAPPER"] = ""  # Metadata needs rustc, not the fixture's placeholder sccache.
        for directory in [self.worktree, self.worktree / "src"]:
            with self.subTest(directory=directory):
                result = subprocess.run(
                    ["cargo", "metadata", "--no-deps", "--format-version", "1"],
                    cwd=directory, env=env, text=True, capture_output=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                metadata = json.loads(result.stdout)
                self.assertEqual(metadata["target_directory"], str(self.main / "target"))

    def test_checksum_mismatch_does_not_install_executable(self):
        self.tool.unlink()
        with patch.object(setup, "download", side_effect=[b"untrusted archive", b"0" * 64]):
            with self.assertRaisesRegex(RuntimeError, "checksum"):
                setup.install(self.main)
        self.assertFalse(self.tool.exists())


if __name__ == "__main__":
    unittest.main()
