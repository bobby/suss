#!/usr/bin/env python3
"""Validate exact fresh primary source nodes; no native support is inferred."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["empty-vector", "vector", "empty-map", "map", "empty-set", "set",
          "nested", "local-child", "quoted-child", "metadata-vector",
          "effect-child", "factory-vector", "factory-map", "factory-set"]


def node(value, depth=0, budget=None):
    if budget is None:
        budget = [4096]
    budget[0] -= 1
    if depth > 8 or budget[0] < 0:
        raise ValueError("source node projection bound exceeded")
    if type(value) is not list or len(value) != 3:
        raise ValueError("invalid source node")
    record, literal, edges = value
    if type(record) is not list or len(record) != 5:
        raise ValueError("invalid source fields")
    for key, field in zip(["op", "tag", "form", "children", "literal?"], record):
        if (type(field) is not list or len(field) != 3 or field[0] != key
                or type(field[1]) is not bool):
            raise ValueError("invalid source field")
        if not field[1] and field[2] is not None:
            raise ValueError("absent source field has a value")
        validate_data(field[2])
    if (type(literal) is not list or len(literal) != 3
            or any(type(x) is not bool for x in literal[:2])):
        raise ValueError("invalid constant value fields")
    if not literal[1] and literal[2] is not None:
        raise ValueError("nonconstant serialized as literal data")
    validate_data(literal[2])
    if type(edges) is not list:
        raise ValueError("invalid source child edges")
    keys = []
    for edge in edges:
        if (type(edge) is not list or len(edge) != 3 or type(edge[0]) is not str
                or edge[1] is not True or type(edge[2]) is not list or len(edge[2]) != 2):
            raise ValueError("invalid source edge")
        key, _, child = edge
        keys.append(key)
        if child[0] == "many":
            if type(child[1]) is not list:
                raise ValueError("invalid child vector")
            for item in child[1]:
                node(item, depth + 1, budget)
        elif child[0] == "one":
            node(child[1], depth + 1, budget)
        else:
            raise ValueError("invalid source edge cardinality")
    declared = record[3]
    expected = ["vector", [["keyword", ":" + key] for key in keys]]
    if declared[1]:
        if declared[2] != expected:
            raise ValueError("declared children differ from projected edges")
    elif keys:
        raise ValueError("undeclared source children")
    return value


def compare(corpus, rows, effects):
    fields(corpus, "schema upstream cases effects")
    if type(corpus["schema"]) is not int or corpus["schema"] != 1 or corpus["upstream"] != PIN:
        raise ValueError("invalid source corpus identity")
    if (type(effects) is not int or effects != 1
            or type(corpus["effects"]) is not int or corpus["effects"] != 1):
        raise ValueError("source effects were replayed or missing")
    for cases in [corpus["cases"], rows]:
        if type(cases) is not list or len(cases) != len(LABELS):
            raise ValueError("invalid source case count")
        for label, row in zip(LABELS, cases):
            if type(row) is not list or len(row) != 2 or row[0] != label:
                raise ValueError("invalid source case identity")
            node(row[1])
    if json.dumps(corpus["cases"]) != json.dumps(rows):
        raise ValueError("fresh primary source observations differ")


def main():
    corpus = parse((ROOT / "tests/oracle/collection-source-ast-observations.json").read_text())
    rows = [parse(line) for line in (ROOT / "tests/oracle/out/collection-source-ast-calls.jsonl").read_text().splitlines()]
    executed = parse((ROOT / "tests/oracle/out/collection-source-ast-results.json").read_text())
    if type(executed) is not list or len(executed) != 2:
        raise ValueError("invalid executed source projection")
    compare(corpus, rows, executed[1])
    compare(corpus, executed[0], executed[1])
    print("14 fresh source AST traces match executed Node projections; effects=1; native conformance unproven")


if __name__ == "__main__":
    main()
