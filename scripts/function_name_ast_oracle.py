#!/usr/bin/env python3
"""Compare fresh actual name-binding traces with Node and the frozen corpus."""
import json
from pathlib import Path
from declaration_environment_oracle import parse
from oracle_transport import PIN, fields

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["anonymous", "named", "shadowed", "tagged", "false-tag", "nil-tag", "multiple-methods"]


def observations(rows):
    if type(rows) is not list or len(rows) != len(LABELS):
        raise ValueError("invalid name case count")
    for label, row in zip(LABELS, rows):
        if type(row) is not list or len(row) != 2 or row[0] != label:
            raise ValueError("invalid name case identity")
        values = row[1]
        if type(values) is not list or len(values) != 21:
            raise ValueError("invalid name field count")
        for index in [0, 1, 2, 3, 9, 11, 13, 14, 16, 18, 20]:
            if type(values[index]) is not bool:
                raise ValueError("invalid name presence or identity")
        if values[10] is not None and type(values[10]) is not bool:
            raise ValueError("invalid self-name value")
        if values[4] not in (["methods"], ["local", "methods"]):
            raise ValueError("invalid genuine name child edges")
        for index in [5, 6, 7, 8, 12, 15]:
            if values[index] is not None and type(values[index]) is not str:
                raise ValueError("invalid symbolic name field")
        if type(values[19]) is not list or any(type(name) is not str for name in values[19]):
            raise ValueError("invalid source parent scopes")
        if values[17] is not None and type(values[17]) not in (str, bool):
            raise ValueError("invalid raw return tag")


def compare(corpus, rows, executed):
    fields(corpus, "schema upstream cases")
    if type(corpus["schema"]) is not int or corpus["schema"] != 1 or corpus["upstream"] != PIN:
        raise ValueError("invalid name corpus identity")
    observations(rows)
    if type(executed) is not list or len(executed) != len(LABELS):
        raise ValueError("invalid executed name cases")
    for case in executed:
        if (type(case) is not list or len(case) != 2
                or type(case[1]) not in (int, float) or case[1] != 42):
            raise ValueError("invalid executed name result")
    observations([case[0] for case in executed])
    if json.dumps(rows) != json.dumps([case[0] for case in executed]):
        raise ValueError("analyzer and Node raw name observations differ")
    if json.dumps(corpus["cases"]) != json.dumps(executed):
        raise ValueError("fresh name observations differ from recorded corpus")


def main():
    corpus = parse((ROOT / "tests/oracle/function-name-ast-observations.json").read_text())
    rows = [parse(line) for line in (ROOT / "tests/oracle/out/function-name-ast-calls.jsonl").read_text().splitlines()]
    executed = parse((ROOT / "tests/oracle/out/function-name-ast-results.json").read_text())
    compare(corpus, rows, executed)
    print("7 fresh name traces match raw Node and corpus; all executions return42; native checked separately")


if __name__ == "__main__":
    main()
