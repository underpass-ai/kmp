import copy
import unittest

from contracts import candidates
from test_formation import GOOD, BAD, SOURCE
from verification import normalize_verification, verification_schema


class BoundVerificationTests(unittest.TestCase):
    def setUp(self):
        self.proposed, _ = candidates({'memories': [GOOD, BAD]}, SOURCE)
        self.ids = [p['id'] for p in self.proposed]

    def test_reordered_keys_preserve_identity_and_rejection(self):
        result = {'verdicts': {self.ids[1]: {'supported': False, 'reason': 'Unsupported claim.'},
                               self.ids[0]: {'supported': True, 'reason': 'Supported claim.'}}}
        normalized = normalize_verification(result, self.proposed)
        self.assertEqual([v['id'] for v in normalized['verdicts']], self.ids)
        self.assertEqual([v['supported'] for v in normalized['verdicts']], [True, False])

    def test_missing_foreign_duplicated_list_and_nonboolean_outputs_are_refused(self):
        valid = {'verdicts': {identity: {'supported': True, 'reason': 'Source says this.'} for identity in self.ids}}
        missing = copy.deepcopy(valid)
        del missing['verdicts'][self.ids[1]]
        foreign = copy.deepcopy(valid)
        foreign['verdicts'][self.ids[1][:-4]] = foreign['verdicts'].pop(self.ids[1])
        nonbool = copy.deepcopy(valid)
        nonbool['verdicts'][self.ids[0]]['supported'] = 'true'
        old_list = {'verdicts': [{'id': self.ids[0], 'supported': True, 'reason': 'Same ID twice.'}] * 2}
        for value in (missing, foreign, nonbool, old_list):
            with self.assertRaises(ValueError):
                normalize_verification(value, self.proposed)

    def test_decoder_has_exact_required_candidate_keys_and_no_foreign_keys(self):
        inner = verification_schema(self.proposed)['properties']['verdicts']
        self.assertEqual(inner['required'], self.ids)
        self.assertEqual(set(inner['properties']), set(self.ids))
        self.assertFalse(inner['additionalProperties'])
        with self.assertRaises(ValueError):
            verification_schema([self.proposed[0], self.proposed[0]])


if __name__ == '__main__':
    unittest.main()
