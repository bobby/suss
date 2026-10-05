#!/usr/bin/env python3
"""Check genuine control AST observations against actual pinned execution."""
import copy
import json
import re
from pathlib import Path

from collection_source_asts_oracle import node
from declaration_environment_oracle import parse
from oracle_transport import PIN, fields

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["empty-do", "single-do", "multi-do", "if-true", "if-false",
          "if-implicit-nil", "let-local", "let-sequential", "special-let",
          "loop-local", "loop-recur", "fixed-function", "variadic-function",
          "try-catch", "try-finally"]


# Upstream gensym uses its compilation process's counter. Preserve raw frozen
# and fresh observations; alpha-check only the two private catch payload forms.
PAYLOAD_PATHS = [
    (13, 1, 2, 1, 2, 1, 0, 2, 2, 1, 1, 1, 1),
    (13, 1, 2, 1, 2, 1, 2, 0, 2, 1, 0, 2, 0, 2, 1, 0, 2, 2),
]


def catch_alpha_view(cases):
    result = copy.deepcopy(cases)
    symbols = []
    for path in PAYLOAD_PATHS:
        value = result
        for index in path:
            value = value[index]
        if (type(value) is not list or len(value) != 2 or value[0] != "symbol"
                or type(value[1]) is not str or not re.fullmatch(r"e[0-9]+", value[1])):
            raise ValueError("invalid private reference catch payload")
        symbols.append(value)
    if symbols[0] != symbols[1]:
        raise ValueError("inconsistent reference catch payload")
    original = copy.deepcopy(symbols[0])

    def count(value):
        if value == original:
            return 1
        if type(value) is list:
            return sum(count(item) for item in value)
        return 0

    if count(result) != 2:
        raise ValueError("unexpected extra reference catch payload use")
    for symbol in symbols:
        symbol[1] = "e16497"
    return result


def compare_raw(rows, executed):
    if json.dumps(rows) != json.dumps(executed):
        raise ValueError("analyzer and Node raw observations differ")


def compare(corpus, rows, effects):
    fields(corpus, "schema upstream cases effects")
    if (type(corpus["schema"]) is not int or corpus["schema"] != 1
            or corpus["upstream"] != PIN):
        raise ValueError("invalid control corpus identity")
    if (type(effects) is not int or effects != 2
            or type(corpus["effects"]) is not int or corpus["effects"] != 2):
        raise ValueError("control cleanup effects differ")
    for cases in [corpus["cases"], rows]:
        if type(cases) is not list or len(cases) != len(LABELS):
            raise ValueError("invalid control case count")
        for label, row in zip(LABELS, cases):
            if type(row) is not list or len(row) != 2 or row[0] != label:
                raise ValueError("invalid control case identity")
            node(row[1])
    if (json.dumps(catch_alpha_view(corpus["cases"]))
            != json.dumps(catch_alpha_view(rows))):
        raise ValueError("fresh control source observations differ")


def main():
    corpus = parse((ROOT / "tests/oracle/control-source-ast-observations.json").read_text())
    rows = [parse(line) for line in (ROOT / "tests/oracle/out/control-source-ast-calls.jsonl").read_text().splitlines()]
    executed = parse((ROOT / "tests/oracle/out/control-source-ast-results.json").read_text())
    if type(executed) is not list or len(executed) != 2:
        raise ValueError("invalid executed control projection")
    compare(corpus, rows, executed[1])
    compare(corpus, executed[0], executed[1])
    compare_raw(rows, executed[0])
    print("15 fresh control AST traces exactly match Node; recorded corpus agrees with one checked catch alpha correspondence; effects=2; native checked separately")


if __name__ == "__main__":
    main()
