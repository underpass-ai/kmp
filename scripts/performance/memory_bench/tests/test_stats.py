"""Statistics, power and the counted PRNG against reference values."""
import unittest

from ..domain import power, stats
from ..domain.prng import SplitMix64, derive_seed, draw


class PrngTest(unittest.TestCase):
    def test_reference_vector(self):
        stream = SplitMix64(0)
        self.assertEqual([stream.next_u64() for _ in range(3)],
                         [0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4, 0x06C45D188009454F])
        self.assertEqual(stream.counter, 3)

    def test_counted_streams_replay_from_any_position(self):
        full = SplitMix64(7)
        values = [full.next_u64() for _ in range(10)]
        self.assertEqual(SplitMix64(7, counter=6).next_u64(), values[6])
        self.assertEqual(draw(7, 9), values[9])

    def test_draw_helpers_are_deterministic_and_in_range(self):
        a, b = SplitMix64(11), SplitMix64(11)
        self.assertEqual(a.shuffled(range(20)), b.shuffled(range(20)))
        self.assertEqual(sorted(SplitMix64(3).shuffled(range(20))), list(range(20)))
        self.assertTrue(all(0 <= SplitMix64(5, i).below(3) < 3 for i in range(50)))
        self.assertTrue(all(0.0 <= SplitMix64(5, i).random() < 1.0 for i in range(50)))
        sample = SplitMix64(13).sample(range(100), 10)
        self.assertEqual(len(set(sample)), 10)
        self.assertIn(SplitMix64(1).choice('xyz'), 'xyz')
        with self.assertRaises(ValueError):
            SplitMix64(1).below(0)
        with self.assertRaises(ValueError):
            SplitMix64(1).choice([])

    def test_forks_are_independent_of_draw_order(self):
        stream = SplitMix64(7)
        before = stream.fork('pool').next_u64()
        stream.next_u64()
        self.assertEqual(stream.fork('pool').next_u64(), before)
        self.assertNotEqual(derive_seed(7, 'a'), derive_seed(7, 'b'))

    def test_golden_shuffle(self):
        """Pins the stream across Python versions: any change here is a BENCH_VERSION bump."""
        self.assertEqual(SplitMix64(7).shuffled(range(10)), GOLDEN_SHUFFLE_SEED_7)


GOLDEN_SHUFFLE_SEED_7 = [8, 1, 5, 9, 0, 4, 3, 2, 6, 7]


class WilsonTest(unittest.TestCase):
    def test_reference_values(self):
        lo, hi = stats.wilson(8, 10)
        self.assertAlmostEqual(lo, 0.4901624715, places=9)
        self.assertAlmostEqual(hi, 0.9433178486, places=9)
        lo, hi = stats.wilson(0, 10)
        self.assertEqual(lo, 0.0)
        self.assertAlmostEqual(hi, 0.2775327998, places=9)
        self.assertIsNone(stats.wilson(0, 0))
        with self.assertRaises(ValueError):
            stats.wilson(3, 2)


class McNemarTest(unittest.TestCase):
    def test_spec_reference_values(self):
        self.assertEqual(stats.mcnemar_exact(5, 0).p_value, 0.0625)
        self.assertEqual(stats.mcnemar_exact(6, 0).p_value, 0.03125)
        self.assertEqual(stats.mcnemar_exact(0, 6).p_value, 0.03125)
        self.assertEqual(stats.mcnemar_exact(0, 0).p_value, 1.0)
        self.assertEqual(stats.mcnemar_exact(3, 3).p_value, 1.0)
        self.assertAlmostEqual(stats.mcnemar_exact(10, 2).p_value, 0.0385742188, places=9)

    def test_from_pairs(self):
        result = stats.mcnemar_from_pairs([(False, True)] * 6 + [(True, True)] * 20 + [(False, False)] * 4)
        self.assertEqual((result.improved, result.worsened, result.discordant), (6, 0, 6))
        self.assertEqual(result.p_value, 0.03125)


class ClopperPearsonTest(unittest.TestCase):
    def test_certifying_high(self):
        self.assertGreaterEqual(stats.clopper_pearson_lower(45, 45, 0.1), 0.95)
        self.assertLess(stats.clopper_pearson_lower(44, 44, 0.1), 0.95)
        self.assertEqual([stats.samples_to_certify(e) for e in (0, 1, 2)], [45, 77, 105])
        self.assertTrue(stats.certifies(76, 77))
        self.assertFalse(stats.certifies(75, 76))

    def test_two_sided_reference(self):
        # 95 % two-sided Clopper-Pearson for 8/10 is (0.4439, 0.9748).
        self.assertAlmostEqual(stats.clopper_pearson_lower(8, 10, 0.025), 0.4439045, places=6)
        self.assertAlmostEqual(stats.clopper_pearson_upper(8, 10, 0.025), 0.9747893, places=6)
        self.assertEqual(stats.clopper_pearson_lower(0, 10), 0.0)
        self.assertEqual(stats.clopper_pearson_upper(10, 10), 1.0)


class BootstrapTest(unittest.TestCase):
    def test_seeded_bootstrap_is_reproducible(self):
        baseline = [i % 3 == 0 for i in range(60)]
        candidate = [i % 2 == 0 for i in range(60)]
        first = stats.paired_bootstrap(baseline, candidate, b=2000, seed=7)
        self.assertEqual(first, stats.paired_bootstrap(baseline, candidate, b=2000, seed=7))
        self.assertNotEqual(first, stats.paired_bootstrap(baseline, candidate, b=2000, seed=11))
        self.assertAlmostEqual(first.estimate, 30 / 60 - 20 / 60)
        self.assertLess(first.lo, first.estimate)
        self.assertGreater(first.hi, first.estimate)

    def test_default_b_and_identical_arms(self):
        same = stats.paired_bootstrap([1, 0, 1], [1, 0, 1])
        self.assertEqual((same.estimate, same.lo, same.hi, same.b), (0.0, 0.0, 0.0, 10_000))
        with self.assertRaises(ValueError):
            stats.paired_bootstrap([1], [1, 0])

    def test_quantile_bootstrap(self):
        self.assertEqual(stats.quantile([4, 1, 3, 2], 0.5), 2.5)
        self.assertEqual(stats.quantile([1, 2, 3, 4, 5], 0.95), 4.8)
        latencies = list(range(1, 101))
        p95 = stats.bootstrap_ci(latencies, lambda v: stats.quantile(v, 0.95), b=500, seed=3)
        self.assertAlmostEqual(p95.estimate, 95.05)
        self.assertLessEqual(p95.lo, p95.estimate)


class ExponentTest(unittest.TestCase):
    def test_student_t(self):
        self.assertAlmostEqual(stats.student_t_ppf(0.975, 1), 12.7062047, places=5)
        self.assertAlmostEqual(stats.student_t_ppf(0.975, 10), 2.2281389, places=6)
        self.assertAlmostEqual(stats.student_t_ppf(0.025, 10), -2.2281389, places=6)

    def test_exact_power_law(self):
        exact = stats.loglog_exponent([1e3, 1e4, 1e5], [2e3, 2e5, 2e7])
        self.assertAlmostEqual(exact.b, 2.0)
        self.assertAlmostEqual(exact.lo, 2.0)
        self.assertAlmostEqual(exact.hi, 2.0)
        two = stats.loglog_exponent([1, 10], [1, 10])
        self.assertIsNone(two.lo)

    def test_noisy_linear_scaling_has_an_interval(self):
        result = stats.loglog_exponent([1e3, 1e4, 1e5] * 2, [1, 10.5, 99, 1.1, 9.8, 101])
        self.assertLess(result.lo, result.b)
        self.assertLess(result.b, result.hi)
        self.assertLess(result.lo, 1.0)
        self.assertGreater(result.hi, 1.0)
        with self.assertRaises(ValueError):
            stats.loglog_exponent([1, 1], [1, 2])


class PowerTest(unittest.TestCase):
    TABLE = {(30, 0.08): 14, (30, 0.30): 28, (60, 0.08): 10, (60, 0.30): 20, (100, 0.08): 8,
             (100, 0.30): 15, (200, 0.08): 5.6, (200, 0.30): 11, (700, 0.08): 3, (700, 0.30): 5.8}

    def test_coefficient_is_the_spec_2_8(self):
        self.assertAlmostEqual(power.coefficient(), 2.8016, places=4)

    def test_spec_table(self):
        for (n, d), points in self.TABLE.items():
            with self.subTest(n=n, d=d):
                self.assertAlmostEqual(power.mde(n, d) * 100, points, delta=0.6)

    def test_three_points_need_hundreds_of_questions(self):
        self.assertTrue(500 <= power.required_n(0.03, 0.08) <= 900)
        self.assertLessEqual(power.mde(power.required_n(0.05, 0.3), 0.3), 0.05)

    def test_indecidible(self):
        weak = power.assess('useful_rate', 30, 0.3, 0.05)
        self.assertFalse(weak.decidable)
        self.assertEqual(power.verdict_or_indecidible(weak, 'neutral'), power.INDECIDIBLE)
        strong = power.assess('useful_rate', 700, 0.08, 0.05)
        self.assertTrue(strong.decidable)
        self.assertEqual(power.verdict_or_indecidible(strong, 'mejora'), 'mejora')
        unknown = power.assess('useful_rate', 30, None, 0.05)
        self.assertEqual(unknown.as_dict()['mde'], None)
        self.assertFalse(unknown.decidable)


if __name__ == '__main__':
    unittest.main()
