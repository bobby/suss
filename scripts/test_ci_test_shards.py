import unittest
from ci_test_shards import (partitions, commands, unit_packages, listed_tests,
                            unit_partitions, unit_commands)


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
        self.assertEqual(result, [["cargo", "test", "--workspace", "--locked", "--no-fail-fast",
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


class UnitShardTests(unittest.TestCase):
    def test_unit_packages_are_workspace_members_with_lib_or_bin_tests(self):
        metadata = ShardTests().metadata()
        metadata["packages"][1]["targets"].append({"kind": ["bin"], "name": "tool", "test": True})
        self.assertEqual(unit_packages(metadata), ["first", "second"])

    def test_listing_keeps_only_tests(self):
        output = "a::one: test\na::bench: benchmark\nb::two: test\n\n3 tests, 0 benchmarks\n"
        self.assertEqual(listed_tests(output), ["a::one", "b::two"])

    def test_every_unit_test_runs_exactly_once(self):
        tests = [("first", f"m::t{i}") for i in range(11)] + [("second", "x")]
        shards = unit_partitions(tests, 4)
        flattened = [test for shard in shards for test in shard]
        self.assertEqual(sorted(flattened), sorted(tests))
        self.assertLessEqual(max(map(len, shards)) - min(map(len, shards)), 1)
        with self.assertRaises(ValueError):
            unit_partitions(tests + [tests[0]], 4)
        with self.assertRaises(ValueError):
            unit_partitions([], 4)

    def test_unit_commands_select_exact_names_per_package(self):
        result = list(unit_commands([("second", "x"), ("first", "b"), ("first", "a")]))
        self.assertEqual(result, [
            ["cargo", "test", "--locked", "-p", "first", "--lib", "--bins", "--",
             "--exact", "--test-threads=2", "a", "b"],
            ["cargo", "test", "--locked", "-p", "second", "--lib", "--bins", "--",
             "--exact", "--test-threads=2", "x"]])
