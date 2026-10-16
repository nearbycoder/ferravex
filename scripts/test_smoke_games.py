"""A valid flat-color game must pass the same smoke gate on Mesa and hardware."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('smoke', Path(__file__).with_name('smoke-games.py'))
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)

class CaptureTests(unittest.TestCase):
    background = bytes([10, 20, 30, 255])
    palette = [bytes([50, 60, 70, 255]), bytes([90, 100, 110, 255]), bytes([150, 160, 170, 255])]

    def test_accepts_small_authored_palette_with_substantial_geometry(self):
        samples = [self.background] * 800 + [self.palette[i % 3] for i in range(200)]
        result = smoke.validate_samples(iter(samples))
        self.assertEqual(result['sampled_colors'], 4)
        self.assertAlmostEqual(result['foreground_fraction'], 0.2)

    def test_rejects_clear_only_and_tiny_error_marks(self):
        for samples in ([self.background] * 1000, [self.background] * 997 + self.palette):
            with self.subTest(colors=len(set(samples))), self.assertRaisesRegex(ValueError, 'blank'):
                smoke.validate_samples(iter(samples))

    def test_rejects_transparent_output_even_with_color_variation(self):
        with self.assertRaisesRegex(ValueError, 'blank'):
            smoke.validate_samples(bytes([i, i, i, 0]) for i in range(200))

if __name__ == '__main__':
    unittest.main()
