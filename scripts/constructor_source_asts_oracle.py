#!/usr/bin/env python3
"""Compare genuine constructor child records with executed pinned observations."""
import json
from pathlib import Path

from collection_source_asts_oracle import node
from declaration_environment_oracle import parse
from oracle_transport import PIN, fields

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["explicit-new", "shorthand-new", "local-new", "empty-new", "qualified-new",
          "current-type-metadata", "foreign-source-hint", "primitive-source-hint",
          "preserved-type-metadata"]
VALUES = [[1, 2], [3, 4], [5, 6], [], [7, 8], [9, 10], [11, 12], [13, 14], [15, 16]]
EFFECTS = list(range(1, 9))


def validate(rows, executed):
    if type(rows) is not list or len(rows) != len(LABELS):
        raise ValueError("invalid constructor observation count")
    for label, row in zip(LABELS, rows):
        if type(row) is not list or len(row) != 3 or row[0] != label:
            raise ValueError("invalid constructor observation identity")
        ast = node(row[1])
        if ast[0][0] != ["op", True, ["keyword", ":new"]]:
            raise ValueError("missing genuine constructor operation")
        if [edge[0] for edge in ast[2]] != ["class", "args"]:
            raise ValueError("constructor child order differs")
        if [edge[2][0] for edge in ast[2]] != ["one", "many"]:
            raise ValueError("constructor child cardinality differs")
        expected = [["type", True, True], ["num-fields", True, 0 if label == "empty-new" else 2], ["record", True, False]]
        expected.append(["private", label == "preserved-type-metadata", True if label == "preserved-type-metadata" else None])
        if label == "local-new":
            expected = [[key, False, None] for key in ["type", "num-fields", "record", "private"]]
        if json.dumps(row[2]) != json.dumps(expected):
            raise ValueError("constructor class declaration facts differ")
    if (type(executed) is not list or len(executed) != 2
            or type(executed[0]) is not list or len(executed[0]) != len(LABELS)):
        raise ValueError("invalid executed constructor observations")
    for index, (row, value) in enumerate(zip(rows, executed[0])):
        if (type(value) is not list or len(value) != 2
                or json.dumps(value[0]) != json.dumps(row)
                or value[1] != VALUES[index]
                or any(type(item) is not int for item in value[1])):
            raise ValueError("constructor analyzer/Node data or fields differ")
    if executed[1] != EFFECTS or any(type(item) is not int for item in executed[1]):
        raise ValueError("constructor effects were reordered, replayed or missing")


def compare(corpus, rows, executed):
    fields(corpus, "schema upstream cases values effects")
    if (type(corpus["schema"]) is not int or corpus["schema"] != 1
            or corpus["upstream"] != PIN):
        raise ValueError("invalid constructor corpus identity")
    validate(rows, executed)
    validate(corpus["cases"], [list(map(list, zip(corpus["cases"], corpus["values"]))), corpus["effects"]])
    if json.dumps(corpus["cases"]) != json.dumps(rows):
        raise ValueError("fresh constructor observations differ from the corpus")


def main():
    rows = [parse(line) for line in (ROOT / "tests/oracle/out/constructor-source-ast-calls.jsonl").read_text().splitlines()]
    executed = parse((ROOT / "tests/oracle/out/constructor-source-ast-results.json").read_text())
    corpus = parse((ROOT / "tests/oracle/constructor-source-ast-observations.json").read_text())
    compare(corpus, rows, executed)
    print("9 constructor AST traces match actual Node data and frozen corpus; field values and eight ordered effects agree; native checked separately")


if __name__ == "__main__":
    main()
