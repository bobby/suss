#!/usr/bin/env python3
"""Run a complete, disjoint partition of Cargo integration-test targets, or with
--unit a partition of every library/binary unit test by exact name.

Examples and doctests are executed by the foundation job. Discover targets from
locked Cargo metadata (and unit tests from each harness's own --list) so new
suites or tests cannot be silently omitted. Invoke Cargo (rather than
executables) to preserve its test env.
"""
import argparse
import json
import subprocess


def partitions(metadata, count):
    if count < 1:
        raise ValueError("shard count must be positive")
    members = set(metadata["workspace_members"])
    targets = sorted(
        (package["name"], target["name"])
        for package in metadata["packages"]
        if package["id"] in members
        for target in package["targets"]
        if target["kind"] == ["test"] and target["test"]
    )
    if not targets or len(targets) != len(set(targets)):
        raise ValueError("missing or duplicate integration-test targets")
    # Cargo selects --test names across all workspace packages. Keep names shared
    # by multiple packages in one shard so those suites still execute exactly once.
    names = sorted({name for _, name in targets})
    owner = {name: index % count for index, name in enumerate(names)}
    return [[target for target in targets if owner[target[1]] == index]
            for index in range(count)]


def commands(targets):
    # Finish every selected harness while Cargo retains a failing exit status.
    command = ["cargo", "test", "--workspace", "--locked", "--no-fail-fast"]
    for target in sorted({name for _, name in targets}):
        command.extend(["--test", target])
    yield command + ["--", "--test-threads=2"]


def unit_packages(metadata):
    members = set(metadata["workspace_members"])
    return sorted(
        package["name"]
        for package in metadata["packages"]
        if package["id"] in members
        and any(target["test"] and set(target["kind"]) & {"lib", "bin"}
                for target in package["targets"])
    )


def listed_tests(output):
    # libtest --list --format=terse prints one "name: test" line per test.
    return sorted({line[:-len(": test")] for line in output.splitlines()
                   if line.endswith(": test")})


def unit_partitions(tests, count):
    """Disjoint round-robin partition of (package, exact test name) pairs."""
    if count < 1:
        raise ValueError("shard count must be positive")
    ordered = sorted(set(tests))
    if not ordered or len(ordered) != len(tests):
        raise ValueError("missing or duplicate unit tests")
    return [ordered[index::count] for index in range(count)]


def unit_commands(tests):
    by_package = {}
    for package, name in tests:
        by_package.setdefault(package, []).append(name)
    for package in sorted(by_package):
        yield (["cargo", "test", "--locked", "-p", package, "--lib", "--bins", "--",
                "--exact", "--test-threads=2"] + sorted(by_package[package]))


def discover_unit_tests(metadata):
    tests = []
    for package in unit_packages(metadata):
        result = subprocess.run(
            ["cargo", "test", "--locked", "-p", package, "--lib", "--bins", "--",
             "--list", "--format=terse"],
            check=True, capture_output=True, text=True,
        )
        tests.extend((package, name) for name in listed_tests(result.stdout))
    return tests


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, required=True)
    parser.add_argument("--index", type=int, required=True)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--unit", action="store_true",
                        help="partition library/binary unit tests by exact name")
    args = parser.parse_args()
    if not 0 <= args.index < args.count:
        parser.error("index must be in [0, count)")
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version=1"],
        check=True, capture_output=True, text=True,
    )
    metadata = json.loads(result.stdout)
    if args.unit:
        shards = unit_partitions(discover_unit_tests(metadata), args.count)
        tests = shards[args.index]
        if not tests:
            parser.error("empty shard; reduce count")
        print(json.dumps({"index": args.index, "count": args.count,
                          "total_tests": sum(map(len, shards)), "tests": len(tests)}), flush=True)
        for command in unit_commands(tests):
            print(" ".join(command[:10]), f"... {len(command) - 10} exact names", flush=True)
            if not args.plan:
                subprocess.run(command, check=True)
        return
    shards = partitions(metadata, args.count)
    targets = shards[args.index]
    if not targets:
        parser.error("empty shard; reduce count")
    print(json.dumps({"index": args.index, "count": args.count,
                      "total_targets": sum(map(len, shards)), "targets": targets}), flush=True)
    for command in commands(targets):
        print(" ".join(command), flush=True)
        if not args.plan:
            subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
