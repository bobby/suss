import unittest
from cljs_inventory import PIN
from cljs_reviews import validate


IDENTITY = 'runtime:f:1'
HASH = 'a' * 64
RECORDS = [{'id': IDENTITY, 'sha256': HASH, 'kind': 'defn'}]
REVIEW = f'''{{:source-sha256 "{HASH}" :visibility :public
 :arities {{:fixed [1 2] :variadic-min nil}} :dependencies ["cljs.core/seq"]
 :classification :portable :rationale "portable source form"
 :status :unimplemented :adaptation-path nil :alternative nil :tests []}}'''


def overlay(review=REVIEW, identity=IDENTITY):
    return f'{{:schema 1 :upstream-commit "{PIN}" :reviews {{"{identity}" {review}}}}}'


class ReviewOverlayTests(unittest.TestCase):
    def test_empty_overlay_keeps_unreviewed_declarations_unassessed(self):
        source = f'{{:schema 1 :upstream-commit "{PIN}" :reviews {{}}}}'
        self.assertEqual(validate(source, RECORDS), (0, 1))

    def test_valid_review_and_noncallable_arities(self):
        self.assertEqual(validate(overlay(), RECORDS), (1, 0))
        self.assertEqual(validate(overlay(REVIEW.replace('{:fixed [1 2] :variadic-min nil}', 'nil')), [dict(RECORDS[0], kind='def')]), (1, 0))

    def test_stale_commit_hash_and_unknown_declaration_fail(self):
        for source in (overlay().replace(PIN, 'b' * 40), overlay().replace(HASH, 'b' * 64), overlay(identity='runtime:unknown:1')):
            with self.subTest(source=source), self.assertRaises(ValueError):
                validate(source, RECORDS)

    def test_duplicate_entries_and_fields_fail(self):
        sources = [overlay().replace(':tests []', ':tests [] :tests []'),
                   f'{{:schema 1 :upstream-commit "{PIN}" :reviews {{"{IDENTITY}" {REVIEW} "{IDENTITY}" {REVIEW}}}}}']
        for source in sources:
            with self.subTest(source=source), self.assertRaises(ValueError):
                validate(source, RECORDS)

    def test_missing_unknown_and_malformed_fields_fail(self):
        for bad in (REVIEW.replace(':rationale "portable source form"', ''), REVIEW.replace(':tests []', ':test []'), REVIEW.replace(':dependencies ["cljs.core/seq"]', ':dependencies "seq"'), REVIEW.replace(':visibility :public', ':visibility :maybe'), REVIEW.replace(':visibility :public', ':visibility []'), REVIEW.replace(':classification :portable', ':classification ":portable"')):
            with self.subTest(review=bad), self.assertRaises(ValueError):
                validate(overlay(bad), RECORDS)

    def test_invalid_arities_fail(self):
        for arities in ('nil', '{:fixed [2 1] :variadic-min nil}', '{:fixed [1 1] :variadic-min nil}', '{:fixed [true] :variadic-min nil}', '{:fixed [] :variadic-min nil}', '{:fixed [1] :variadic-min "one"}'):
            with self.subTest(arities=arities), self.assertRaises(ValueError):
                validate(overlay(REVIEW.replace('{:fixed [1 2] :variadic-min nil}', arities)), RECORDS)

    def test_adaptation_implementation_and_exclusion_require_evidence(self):
        for review in (REVIEW.replace(':classification :portable', ':classification :adapted'), REVIEW.replace(':status :unimplemented', ':status :implemented'), REVIEW.replace(':status :unimplemented', ':status :excluded')):
            with self.subTest(review=review), self.assertRaises(ValueError):
                validate(overlay(review), RECORDS)
        review = REVIEW.replace(':classification :portable', ':classification :host-specific').replace(':status :unimplemented', ':status :excluded').replace(':alternative nil', ':alternative "Use an explicit typed host binding"')
        self.assertEqual(validate(overlay(review), RECORDS), (1, 0))

    def test_code_and_trailing_data_are_rejected(self):
        for source in (overlay() + ' nil', overlay(REVIEW.replace(':tests []', ':tests (run-tests)')), overlay(REVIEW.replace(':tests []', ':tests #js []'))):
            with self.subTest(source=source), self.assertRaises(ValueError):
                validate(source, RECORDS)


if __name__ == '__main__':
    unittest.main()
