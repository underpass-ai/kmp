import unittest

from ..runtime.driver import next_read


class TraceContinuationTest(unittest.TestCase):
    def test_a_zero_item_trace_page_follows_its_required_bytes_action(self):
        action = {'tool': 'kmp_trace', 'arguments': {'from': 'entry:a', 'budget': {'max_bytes': 20000},
                                                     'page': {'cursor': 'kmpr1:0:ab'}}}
        page = {'trace': [], 'page': {'returned': 0, 'has_more': True, 'required_bytes': 20000},
                'next_actions': [action]}
        self.assertEqual(next_read(page), ('kmp_trace', action['arguments']))
        # A page that returned items, or one that ended, is not followed here.
        self.assertIsNone(next_read({'page': {'returned': 2, 'has_more': True}, 'next_actions': [action]}))
        self.assertIsNone(next_read({'page': {'returned': 0, 'has_more': False}, 'next_actions': []}))
        # A recall's projection.next_action still leads.
        ask = {'projection': {'next_action': {'tool': 'kmp_ask', 'arguments': {'continuation': 'read_x'}}}}
        self.assertEqual(next_read(ask), ('kmp_ask', {'continuation': 'read_x'}))


if __name__ == '__main__':
    unittest.main()
