#!/usr/bin/env python3
"""Strict primary method observations; native execution is checked separately."""
import json
from pathlib import Path
from declaration_environment_oracle import parse
from oracle_transport import PIN, fields

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["plain", "method-recur", "nested-loop", "nested-function",
          "multiple-methods", "variadic", "unselected-recur"]


def observations(rows):
    if type(rows) is not list or len(rows) != len(LABELS):
        raise ValueError("invalid method case count")
    for label, row in zip(LABELS, rows):
        if type(row) is not list or len(row) != 2 or row[0] != label:
            raise ValueError("invalid method case identity")
        methods = row[1]
        arities = 2 if label == "multiple-methods" else 1
        if type(methods) is not list or len(methods) != arities:
            raise ValueError("invalid method cardinality")
        for method in methods:
            if type(method) is not list or len(method) != 6:
                raise ValueError("invalid method fields")
            presence, recurs, body_presence, body, context, has_x = method
            if (presence is not True or recurs is not None and recurs is not True
                    or body_presence is not True or body is not True
                    or context not in ("expr", "statement", "return")
                    or type(has_x) is not bool):
                raise ValueError("invalid method field presence or value")


def compare(corpus, rows, executed):
    fields(corpus, "schema upstream cases")
    if type(corpus["schema"]) is not int or corpus["schema"] != 1 or corpus["upstream"] != PIN:
        raise ValueError("invalid method corpus identity")
    observations(rows)
    if type(executed) is not list or len(executed) != len(LABELS):
        raise ValueError("invalid executed method cases")
    for case in executed:
        if (type(case) is not list or len(case) != 2
                or type(case[1]) not in (int, float) or case[1] != 42):
            raise ValueError("invalid executed method result")
    observations([case[0] for case in executed])
    if json.dumps(rows) != json.dumps([case[0] for case in executed]):
        raise ValueError("analyzer and Node raw observations differ")
    if json.dumps(corpus["cases"]) != json.dumps(executed):
        raise ValueError("fresh method observations differ from recorded corpus")


def main():
    corpus = parse((ROOT / "tests/oracle/method-recurrence-observations.json").read_text())
    rows = [parse(line) for line in (ROOT / "tests/oracle/out/method-recurrence-calls.jsonl").read_text().splitlines()]
    executed = parse((ROOT / "tests/oracle/out/method-recurrence-results.json").read_text())
    compare(corpus, rows, executed)
    print("7 fresh method traces match raw Node and recorded corpus; all executions return42; native checked separately")


if __name__ == "__main__":
    main()
