"""Large-room traversal, seams, palette and reproducibility contracts."""
import unittest

import large_level_chunks as large


MASKS = ("U", "D", "L", "R", "UD", "UL", "UR", "DL", "DR", "LR", "UDL", "UDR", "ULR", "DLR", "UDLR")


class LargeRoomTests(unittest.TestCase):
    def test_every_family_connects_every_port_with_base_movement(self):
        silhouettes = set()
        for family in large.FAMILIES:
            for index, mask in enumerate(MASKS):
                image = large.build(mask, 20261001 + index, family)
                report = large.validate(image, mask)
                self.assertTrue(report.valid, f"{family}/{mask}: {report.errors}")
                self.assertGreater(report.air_fraction, 0.15)
                self.assertLess(report.air_fraction, 0.45)
                silhouettes.add(image.tobytes())
        self.assertEqual(len(silhouettes), len(MASKS) * len(large.FAMILIES))

    def test_seed_reproduces_geometry(self):
        for family in large.FAMILIES:
            first = large.build("UDLR", 716, family)
            second = large.build("UDLR", 716, family)
            self.assertEqual(first.tobytes(), second.tobytes())

    def test_corners_and_blocked_routes_are_rejected(self):
        image = large.build("UD", 918, "winding")
        image.putpixel((0, 0), (0, 0, 0, 0))
        self.assertFalse(large.validate(image, "UD").valid)
        image = large.build("UD", 918, "winding")
        for x in range(large.SIZE):
            image.putpixel((x, large.SIZE // 2), (0, 0, 0, 255))
            image.putpixel((x, large.SIZE // 2 + 1), (0, 0, 0, 255))
        self.assertFalse(large.validate(image, "UD").valid)


if __name__ == "__main__":
    unittest.main()
