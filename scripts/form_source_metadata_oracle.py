#!/usr/bin/env python3
"""Compare bounded actual pinned macro source-metadata projections."""
import json
from pathlib import Path
from oracle_transport import PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["nested", "unicode-crlf", "overrides", "file", "generated", "conditional", "chained", "slash", "tag"]


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate field: {key}")
        result[key] = value
    return result


def parse(text):
    def invalid(value):
        raise ValueError(f"invalid JSON constant: {value}")
    return json.loads(text, object_pairs_hook=unique, parse_constant=invalid)


def validate(cases):
    if type(cases) is not list or len(cases) != len(LABELS):
        raise ValueError("missing or extra source metadata observations")
    for expected, case in zip(LABELS, cases):
        if type(case) is not list or len(case) != 2 or case[0] != expected:
            raise ValueError("changed source metadata observation identity/order")
        rows = case[1]
        if expected == "tag":
            if (type(rows) is not list or len(rows) != 6 or rows[0] != "my.Hint"
                    or any(type(value) is not int or value <= 0 for value in rows[1:5])
                    or rows[5] is not None):
                raise ValueError("invalid source tag metadata")
            continue
        if type(rows) is not list or len(rows) != 4:
            raise ValueError("invalid source metadata rows")
        for index, row in enumerate(rows):
            if type(row) is not list or len(row) != (5 if index == 1 else 6):
                raise ValueError("missing or extra source metadata fields")
            if expected == "generated":
                if any(value is not None for value in row):
                    raise ValueError("invented reader metadata on generated syntax")
                continue
            if expected == "slash" and index == 3:
                if any(value is not None for value in row[:4]):
                    raise ValueError("invented slash symbol locations")
            elif any(type(value) is not int or value <= 0 for value in row[:4]):
                raise ValueError("invalid exact reader position")
            filename = "fixture/app.sus" if expected == "file" else None
            if row[4] != filename:
                raise ValueError("changed reader filename")
            if index != 1 and row[5] is not True:
                raise ValueError("lost explicit source metadata marker")
    return cases


def compare(expected, actual):
    if type(expected) is not dict or set(expected) != {"schema", "upstream", "cases"}:
        raise ValueError("invalid source metadata corpus fields")
    if type(expected["schema"]) is not int or expected["schema"] != 1 or expected["upstream"] != PIN:
        raise ValueError("invalid source metadata schema or upstream pin")
    if validate(expected["cases"]) != validate(actual):
        raise ValueError("fresh pinned source metadata differs")


def main():
    expected = parse((ROOT / "tests/oracle/form-source-metadata-observations.json").read_text())
    actual = [parse(line) for line in (ROOT / "tests/oracle/out/form-source-metadata.jsonl").read_text().splitlines()]
    compare(expected, actual)
    print("9 actual pinned macro source metadata projections match; native execution checked separately")


if __name__ == "__main__":
    main()
