import copy
import unittest
from form_source_metadata_oracle import LABELS, PIN, compare, parse, validate


def cases():
    result = []
    for label in LABELS:
        if label == "tag":
            result.append([label, ["my.Hint", 1, 2, 3, 4, None]])
            continue
        rows = []
        for index in range(4):
            row = [1, 2, 3, 4, "fixture/app.sus" if label == "file" else None]
            if index != 1:
                row.append(True)
            if label == "generated":
                row = [None] * len(row)
            if label == "slash" and index == 3:
                row[:4] = [None] * 4
            rows.append(row)
        result.append([label, rows])
    return result


class SourceMetadataOracle(unittest.TestCase):
    def test_valid_projection_and_exact_comparison(self):
        actual = cases()
        compare({"schema": 1, "upstream": PIN, "cases": actual}, copy.deepcopy(actual))
        changed = copy.deepcopy(actual)
        changed[0][1][0][0] += 1
        with self.assertRaises(ValueError):
            compare({"schema": 1, "upstream": PIN, "cases": actual}, changed)

    def test_boolean_float_string_and_nil_are_not_positions(self):
        for value in [True, 1.0, "1", None, 0, -1]:
            actual = cases()
            actual[0][1][0][0] = value
            with self.assertRaises(ValueError):
                validate(actual)

    def test_invented_generated_location_and_wrong_filename_fail(self):
        for index, field, value in [(4, 0, 1), (3, 4, None), (0, 4, "fixture/app.sus")]:
            actual = cases()
            actual[index][1][0][field] = value
            with self.assertRaises(ValueError):
                validate(actual)

    def test_missing_extra_reordered_and_unknown_observations_fail(self):
        actual = cases()
        for value in [actual[:-1], actual + actual[:1], list(reversed(actual)), [["unknown", actual[0][1]]] + actual[1:]]:
            with self.assertRaises(ValueError):
                validate(value)

    def test_malformed_width_and_nonboolean_marker_fail(self):
        for value in [[1, 2, 3, 4], [1, 2, 3, 4, None, 1]]:
            actual = cases()
            actual[0][1][0] = value
            with self.assertRaises(ValueError):
                validate(actual)

    def test_slash_locations_and_tag_metadata_cannot_be_invented_or_lost(self):
        actual = cases()
        actual[7][1][3][0] = 1
        with self.assertRaises(ValueError):
            validate(actual)
        for field, value in [(0, "wrong.Hint"), (1, None), (2, True), (5, "wrong-file")]:
            actual = cases()
            actual[8][1][field] = value
            with self.assertRaises(ValueError):
                validate(actual)

    def test_schema_pin_and_duplicate_fields_fail(self):
        for schema, pin in [(True, PIN), (2, PIN), (1, "wrong")]:
            with self.assertRaises(ValueError):
                compare({"schema": schema, "upstream": pin, "cases": cases()}, cases())
        for text in ['{"schema":1,"schema":1}', '[NaN]', '[Infinity]']:
            with self.assertRaises(ValueError):
                parse(text)


if __name__ == "__main__":
    unittest.main()
