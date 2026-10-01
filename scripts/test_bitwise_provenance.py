"""The conditional source record must stay pinned and cover its whole form."""
from copy import deepcopy
import unittest
import bitwise_provenance as provenance


class BitwiseProvenanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document, cls.source = provenance.check()

    def test_current_conditional_source_matches_pin_and_complete_region(self):
        provenance.verify(self.document, self.source)

    def test_changed_pin_file_hash_and_region_hash_are_rejected(self):
        for field in ['upstream', 'source-file-sha256']:
            document = deepcopy(self.document)
            document[field] = '0' * 64
            with self.subTest(field=field), self.assertRaises(ValueError):
                provenance.verify(document, self.source)
        document = deepcopy(self.document)
        document['conditional-imul']['source-region-sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'region hash'):
            provenance.verify(document, self.source)

    def test_truncated_or_comment_extended_region_fails_even_with_matching_hash(self):
        for start, end in [(953, 955), (953, 966)]:
            document = deepcopy(self.document)
            region = document['conditional-imul']
            region['line'], region['end-line'] = start, end
            region['source-region-sha256'] = provenance.digest(
                b''.join(self.source.splitlines(keepends=True)[start - 1:end]))
            with self.subTest(end=end), self.assertRaises(ValueError):
                provenance.verify(document, self.source)

    def test_invalid_ranges_and_duplicate_metadata_fields_are_rejected(self):
        for start, end in [(True, 964), (0, 964), (965, 964), (953, 999999)]:
            document = deepcopy(self.document)
            region = document['conditional-imul']
            region['line'], region['end-line'] = start, end
            with self.subTest(start=start, end=end), self.assertRaisesRegex(ValueError, 'range'):
                provenance.verify(document, self.source)
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            provenance.json_data('{"schema":1,"schema":1}')
