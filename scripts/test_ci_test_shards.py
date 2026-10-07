import unittest
from ci_test_shards import partitions, commands


class ShardTests(unittest.TestCase):
    def metadata(self):
        return {"workspace_members": ["a", "b"], "packages": [
            {"id": "a", "name": "first", "targets": [
                {"kind": ["test"], "name": str(i), "test": True} for i in range(17)
            ] + [{"kind": ["lib"], "name": "first", "test": True},
                 {"kind": ["test"], "name": "disabled", "test": False}]},
            {"id": "b", "name": "second", "targets": [
                {"kind": ["test"], "name": "added_later", "test": True}]},
            {"id": "dependency", "name": "dependency", "targets": [
                {"kind": ["test"], "name": "external", "test": True}]}]}

    def test_all_workspace_suites_once_and_new_suites_discovered(self):
        shards = partitions(self.metadata(), 8)
        flattened = [target for shard in shards for target in shard]
        self.assertEqual(len(flattened), 18)
        self.assertEqual(len(set(flattened)), 18)
        self.assertIn(("second", "added_later"), flattened)
        self.assertLessEqual(max(map(len, shards)) - min(map(len, shards)), 1)
        changed = self.metadata()
        changed["packages"].reverse()
        self.assertEqual(shards, partitions(changed, 8))

    def test_cargo_commands_keep_complete_suites_and_bounded_workers(self):
        result = list(commands([("first", "a"), ("first", "b"), ("second", "c")]))
        self.assertEqual(result, [["cargo", "test", "--workspace", "--locked",
                                  "--test", "a", "--test", "b", "--test", "c",
                                  "--", "--test-threads=2"]])

    def test_same_named_suites_stay_together_across_packages(self):
        metadata = self.metadata()
        metadata["packages"][1]["targets"].append(
            {"kind": ["test"], "name": "0", "test": True})
        shards = partitions(metadata, 8)
        containing = [shard for shard in shards if ("first", "0") in shard]
        self.assertEqual(len(containing), 1)
        self.assertIn(("second", "0"), containing[0])
        command = list(commands(containing[0]))[0]
        self.assertEqual(command.count("0"), 1)

    def test_empty_duplicate_and_invalid_counts_fail(self):
        with self.assertRaises(ValueError):
            partitions(self.metadata(), 0)
        with self.assertRaises(ValueError):
            partitions({"workspace_members": [], "packages": []}, 8)
        duplicate = self.metadata()
        duplicate["packages"][0]["targets"].append(duplicate["packages"][0]["targets"][0])
        with self.assertRaises(ValueError):
            partitions(duplicate, 8)
