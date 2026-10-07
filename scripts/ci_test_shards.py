#!/usr/bin/env python3
"""Run a complete, disjoint partition of Cargo integration-test targets.

Libraries, binaries, examples and doctests are executed by the foundation job.
Discover targets from locked Cargo metadata so new integration suites cannot be
silently omitted. Invoke Cargo (rather than executables) to preserve its test env.
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
    command = ["cargo", "test", "--workspace", "--locked"]
    for target in sorted({name for _, name in targets}):
        command.extend(["--test", target])
    yield command + ["--", "--test-threads=2"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--count", type=int, required=True)
    parser.add_argument("--index", type=int, required=True)
    parser.add_argument("--plan", action="store_true")
    args = parser.parse_args()
    if not 0 <= args.index < args.count:
        parser.error("index must be in [0, count)")
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version=1"],
        check=True, capture_output=True, text=True,
    )
    shards = partitions(json.loads(result.stdout), args.count)
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
