"""Release regressions: immutable tags, retries, version parity and lockfiles."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from bump_version import check, set_version
from prepare_release import release_plan


class ReleasePlanTests(unittest.TestCase):
    def plan(self, version="0.2.0", event="push", ref="refs/heads/main", tags=None, releases=None, retry=False):
        return release_plan(version, event, ref, "abc123", tags or {}, releases or [], retry=retry)

    def test_new_version_creates_own_tag(self):
        self.assertEqual(self.plan(), {"tag": "v0.2.0", "sha": "abc123", "publish": True})

    def test_unchanged_version_skips_even_on_new_commit(self):
        self.assertFalse(self.plan(tags={"v0.2.0": "oldcommit"})["publish"])

    def test_rerun_all_jobs_resumes_after_tag_was_created(self):
        self.assertTrue(self.plan(tags={"v0.2.0": "abc123"}, retry=True)["publish"])

    def test_published_release_is_never_rebuilt(self):
        self.assertFalse(self.plan(event="workflow_dispatch", releases=[
            {"tag_name": "v0.2.0", "draft": False}])["publish"])

    def test_failed_draft_can_be_retried_at_same_commit(self):
        self.assertTrue(self.plan(event="workflow_dispatch", tags={"v0.2.0": "abc123"},
                                 releases=[{"tag_name": "v0.2.0", "draft": True}])["publish"])

    def test_tag_push_is_supported(self):
        self.assertTrue(self.plan(ref="refs/tags/v0.2.0", tags={"v0.2.0": "abc123"})["publish"])

    def test_wrong_tag_fails(self):
        with self.assertRaisesRegex(ValueError, "does not match"):
            self.plan(ref="refs/tags/v0.3.0")

    def test_never_moves_existing_tag(self):
        with self.assertRaisesRegex(ValueError, "another commit"):
            self.plan(event="workflow_dispatch", tags={"v0.2.0": "oldcommit"})

    def test_older_release_cannot_replace_latest(self):
        with self.assertRaisesRegex(ValueError, "must be newer"):
            self.plan(releases=[{"tag_name": "v0.3.0", "draft": False}])

    def test_versions_are_compared_numerically(self):
        self.assertTrue(self.plan(version="0.1.10", releases=[
            {"tag_name": "v0.1.9", "draft": False}])["publish"])


class VersionTests(unittest.TestCase):
    def fixture(self, root, nested=False, pointer=False, engine=False):
        app = root / "app" if nested else root
        (app / "src-tauri").mkdir(parents=True)
        (app / "package.json").write_text('{"name":"example","version":"0.1.0"}')
        (app / "src-tauri/tauri.conf.json").write_text(json.dumps({
            "version": "../package.json" if pointer else "0.1.0"}))
        (app / "src-tauri/Cargo.toml").write_text('[package]\nname = "example"\nversion = "0.1.0"\n')
        (app / "src-tauri/Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "dependency"\nversion = "9.8.7"\n\n'
            '[[package]]\nname = "example"\nversion = "0.1.0"\n')
        (app / "package-lock.json").write_text(json.dumps({
            "version": "0.1.0", "packages": {"": {"version": "0.1.0"},
            "node_modules/dependency": {"version": "9.8.7"}}}))
        if engine:
            (root / "engine/src/aics_engine").mkdir(parents=True)
            (root / "engine/pyproject.toml").write_text('[project]\nname = "engine"\nversion = "0.1.0"\n')
            (root / "engine/uv.lock").write_text('version = 1\n[[package]]\nname = "engine"\nversion = "0.1.0"\n')
            (root / "engine/src/aics_engine/__init__.py").write_text('__version__ = "0.1.0"\n')
        return app

    def test_sync_all_layouts_and_preserve_dependencies(self):
        for nested, pointer, engine in ((True, False, True), (False, True, False),
                                        (False, False, False), (False, False, True)):
            with self.subTest(nested=nested, pointer=pointer, engine=engine), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                app = self.fixture(root, nested, pointer, engine)
                set_version("0.1.10", root)
                self.assertEqual(check(root), "0.1.10")
                cargo = (app / "src-tauri/Cargo.lock").read_text()
                self.assertIn('name = "dependency"\nversion = "9.8.7"', cargo)
                lock = json.loads((app / "package-lock.json").read_text())
                self.assertEqual(lock["packages"]["node_modules/dependency"]["version"], "9.8.7")
                if pointer:
                    self.assertEqual(json.loads((app / "src-tauri/tauri.conf.json").read_text())["version"], "../package.json")

    def test_package_only_bump_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            (root / "package.json").write_text('{"version":"0.1.10"}')
            with self.assertRaisesRegex(ValueError, "Versions differ"):
                check(root)

    def test_invalid_versions_do_not_write_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root)
            for version in ("main", "01.2.3", "1.2.3-beta", "1.2"):
                with self.assertRaises(ValueError):
                    set_version(version, root)
            self.assertEqual(check(root), "0.1.0")


class PrepareIntegrationTests(unittest.TestCase):
    def test_creates_annotated_tag_in_isolated_remote_and_skips_repeat(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo, remote, tools = root / "repo", root / "remote.git", root / "bin"
            repo.mkdir()
            tools.mkdir()
            VersionTests().fixture(repo)
            set_version("0.2.0", repo)
            (repo / "scripts").mkdir()
            for name in ("bump_version.py", "prepare_release.py"):
                shutil.copyfile(Path(__file__).with_name(name), repo / "scripts" / name)

            def git(*args):
                return subprocess.check_output(["git", *args], cwd=repo, text=True, stderr=subprocess.DEVNULL).strip()

            git("init")
            git("config", "user.name", "Release test")
            git("config", "user.email", "release-test@example.invalid")
            git("add", ".")
            git("commit", "-m", "Version 0.2.0")
            git("init", "--bare", str(remote))
            git("remote", "add", "origin", str(remote))
            # Only GitHub's read API is stubbed; tags are really pushed to a local bare repo.
            gh = tools / "gh"
            gh.write_text('#!/bin/sh\nprintf \'[[{"tag_name":"v0.1.0","draft":false}]]\\n\'\n')
            gh.chmod(0o755)
            output = root / "output"
            env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"],
                       GITHUB_REPOSITORY="test/example", GITHUB_EVENT_NAME="push",
                       GITHUB_REF="refs/heads/main", GITHUB_RUN_ATTEMPT="1",
                       GITHUB_OUTPUT=str(output), UPDATER_SIGNING_KEY="test-only")
            command = [sys.executable, "scripts/prepare_release.py"]
            subprocess.run(command, cwd=repo, env=env, check=True, capture_output=True, text=True)
            self.assertIn("publish=true", output.read_text())
            self.assertEqual(git("cat-file", "-t", "v0.2.0"), "tag")
            self.assertIn("refs/tags/v0.2.0", git("ls-remote", "--tags", "origin"))
            output.unlink()
            subprocess.run(command, cwd=repo, env=env, check=True, capture_output=True, text=True)
            self.assertIn("publish=false", output.read_text())
            output.unlink()
            env["GITHUB_RUN_ATTEMPT"] = "2"
            subprocess.run(command, cwd=repo, env=env, check=True, capture_output=True, text=True)
            self.assertIn("publish=true", output.read_text())


if __name__ == "__main__":
    unittest.main()
