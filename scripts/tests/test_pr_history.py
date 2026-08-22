import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "pr_history.py"
SPEC = importlib.util.spec_from_file_location("pr_history", SCRIPT_PATH)
assert SPEC and SPEC.loader
pr_history = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = pr_history
SPEC.loader.exec_module(pr_history)


class CategoryTests(unittest.TestCase):
    def test_feature_title(self) -> None:
        self.assertEqual("feature", pr_history.normalized_category("feat(mcp): add pageable recall"))

    def test_plain_add_title(self) -> None:
        self.assertEqual("feature", pr_history.normalized_category("Add command contract"))

    def test_removal_wins_over_feature_prefix(self) -> None:
        self.assertEqual("removal", pr_history.normalized_category("feat: remove legacy admin API"))

    def test_release_title(self) -> None:
        self.assertEqual("release", pr_history.normalized_category("chore: v0.1.11"))


class NumstatTests(unittest.TestCase):
    def test_text_and_binary_rows(self) -> None:
        parsed = pr_history.parse_numstat("12\t3\tREADME.md\n-\t-\timage.png")
        self.assertEqual((12, 3), parsed["README.md"])
        self.assertEqual((None, None), parsed["image.png"])

    def test_nul_delimited_rename_rows(self) -> None:
        raw = b"2\t1\tREADME.md\x003\t3\t\x00old/path.rs\x00new/path.rs\x00"
        parsed = pr_history.parse_numstat_z(raw)
        self.assertEqual((2, 1), parsed["README.md"])
        self.assertEqual((3, 3), parsed["new/path.rs"])


class MarkdownTests(unittest.TestCase):
    def test_table_escaping(self) -> None:
        self.assertEqual("left\\|right", pr_history.markdown_table_text("left|right"))

    def test_source_relative_links_become_permanent(self) -> None:
        repository = pr_history.REPOSITORIES[1]
        pull_request = pr_history.PullRequest(
            repository=repository,
            number=1,
            title="Test",
            body="Read [the ADR](docs/adr/example.md#decision) and [the PR](https://example.test).",
            url="https://example.test/pr/1",
            author="agent",
            created_at="2026-01-01T00:00:00Z",
            merged_at="2026-01-02T00:00:00Z",
            base_ref="main",
            base_sha="base",
            head_ref="change",
            head_sha="head",
            merge_commit_sha="abc123",
        )
        rendered = pr_history.safe_source_body(pull_request)
        self.assertIn(
            "https://github.com/underpass-ai/kmp/blob/abc123/docs/adr/example.md#decision",
            rendered,
        )
        self.assertIn("https://example.test", rendered)


if __name__ == "__main__":
    unittest.main()
