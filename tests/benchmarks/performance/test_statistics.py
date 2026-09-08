import unittest

from verify import statistics_for


class MeasurementTests(unittest.TestCase):
    def test_nearest_rank_uses_all_samples_including_outlier(self):
        samples = [{'elapsed_ms': float(n), 'correct': True} for n in range(30, 0, -1)]
        samples[0]['elapsed_ms'] = 1000.0
        result = statistics_for(samples, 30)
        self.assertEqual(result['p95_ms'], 29.0)
        self.assertEqual(result['maximum_ms'], 1000.0)
        self.assertEqual(result['samples'], 30)

    def test_missing_or_failed_measurement_cannot_be_reported_as_pass(self):
        samples = [{'elapsed_ms': 1.0, 'correct': True} for _ in range(30)]
        with self.assertRaises(ValueError):
            statistics_for(samples[:-1], 30)
        samples[5]['correct'] = False
        with self.assertRaises(ValueError):
            statistics_for(samples, 30)


if __name__ == '__main__':
    unittest.main()
