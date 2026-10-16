"""Numerical seams, authoring formats, and shipped-output validation."""
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np
from PIL import Image
from scipy import ndimage

import generate as bg
import openings
import synthesis


class BackgroundTests(unittest.TestCase):
    def test_rectangular_map_crop_and_spawn(self):
        rgba = np.full((3, 8, 4), 255, dtype=np.uint8)
        rgba[:, :, 3] = 0
        rgba[1, 2] = (255, 255, 255, 255)
        rgba[1, 3] = (0, 0, 0, 255)
        rgba[2, 3] = (69, 255, 0, 255)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "map.png"
            Image.fromarray(rgba).save(path)
            footprint, spawn = bg.map_layout(path)
        np.testing.assert_array_equal(footprint, [[1, 1], [0, 1]])
        self.assertEqual(spawn, (0, 0))

    def test_depth_artwork_has_pure_end_bands_and_a_mixed_transition(self):
        mask = bg.depth_tundra((1000, 32), (0, 0), 1.0)
        self.assertFalse(mask[:bg.PAD + 4 * bg.CHUNK].any())
        self.assertTrue(mask[bg.PAD + 6 * bg.CHUNK:].all())
        transition = mask[bg.PAD + 4 * bg.CHUNK:bg.PAD + 6 * bg.CHUNK]
        self.assertTrue(transition.any() and not transition.all())

    def test_parallax_depth_bands_follow_the_viewed_world_depth(self):
        shape = (1200, 32)
        center = (shape[0] - 2 * bg.PAD) / 2
        for depth in bg.DEPTHS:
            mask = bg.depth_tundra(shape, (0, 0), depth)
            surface_row = int(center + bg.PAD + (100 - center) / depth)
            deep_row = int(center + bg.PAD + (700 - center) / depth)
            self.assertFalse(mask[surface_row].any())
            self.assertTrue(mask[deep_row].all())

    def test_gaussian_regions_match_whole_image_with_transparency(self):
        raw = np.random.default_rng(719).integers(0, 256, (231, 207, 4), dtype=np.uint8)
        for radius in [3, 5]:
            reference = bg.blur_linear(raw, radius)
            support = int(np.ceil(radius * 1.5))
            for x, y in [(0, 0), (96, 96), (192, 192)]:
                x1, y1 = min(x + 96, 207), min(y + 96, 231)
                x0, y0 = max(0, x - support), max(0, y - support)
                patch = raw[y0:min(231, y1 + support), x0:min(207, x1 + support)]
                actual = bg.blur_linear(patch, radius)[y-y0:y1-y0, x-x0:x1-x0]
                np.testing.assert_array_equal(actual, reference[y:y1, x:x1])

    def test_ldtk_flip_and_priority(self):
        grid = np.array([[0, 0, 0], [0, 1, 2], [0, 0, 0]], dtype=np.int16)
        rule = dict(active=True, size=3, pattern=[0, 0, 0, 2, 1, 0, 0, 0, 0],
                    flipX=True, flipY=False, chance=1, breakOnMatch=True, tileRectsIds=[[13]])
        layer = dict(autoRuleGroups=[dict(name="fog", rules=[rule]),
                                    dict(name="wall", rules=[dict(rule, pattern=[0, 0, 0, 0, 1, 0, 0, 0, 0], tileRectsIds=[[99]])])])
        tiles, flags = bg.select_tiles(grid, layer, np.random.default_rng(1))
        self.assertEqual(int(tiles[1, 1]), 13)
        self.assertEqual(int(flags[1, 1]), 1)

    def test_openings_are_the_artists_spaced_by_whole_walls(self):
        style = bg.Style("ice")
        allowed = np.zeros((300, 260), bool)
        allowed[:200, :] = allowed[:, :100] = True
        planes = openings.plan(allowed, style.libraries, openings.FILL["ice"], bg.DEPTHS, np.random.default_rng(5))
        usable = ndimage.binary_erosion(allowed, np.ones((2 * openings.MARGIN + 1,) * 2), border_value=0)
        spacing = np.ones((2 * openings.GAP + 1,) * 2, bool)
        for plane, library in zip(planes, style.libraries):
            self.assertTrue(plane.any())
            self.assertFalse((plane & ~usable).any(), "openings keep clear of the covered area's edge")
            artist = {shape_key(stamp[::fy, ::fx]) for stamp in library for fy in (1, -1) for fx in (1, -1)}
            labels, _ = ndimage.label(plane, np.ones((3, 3)))
            for index, area in enumerate(ndimage.find_objects(labels), 1):
                own = labels == index
                self.assertIn(shape_key(own[area]), artist, "every opening is one of the artist's")
                self.assertFalse((ndimage.binary_dilation(own, spacing) & plane & ~own).any(),
                                 "walls between openings are at least GAP tiles thick")

    def test_detailing_stays_near_opening_edges(self):
        style = bg.Style("ice")
        exemplar = style.exemplars[1]
        wall = np.ones((120, 150), bool)
        wall[30:50, 20:70] = wall[70:90, 60:130] = False
        grid = synthesis.synthesize(synthesis.Context(wall), exemplar, np.random.default_rng(3))
        distance = synthesis.signed_distance(wall, 32)
        values = set(np.unique(exemplar.values[exemplar.values >= 0]))
        self.assertTrue(set(np.unique(grid)) <= values)
        self.assertTrue(np.all(grid[distance < -synthesis.OPENING_REACH] == 0))
        self.assertTrue(np.all(grid[distance > exemplar.reach] == exemplar.base))
        self.assertTrue(np.any((grid != 0) & (grid != exemplar.base)), "openings receive detailing")

    def test_shipped_assets_are_current_and_have_matching_gutters(self):
        expected_source = bg.source_fingerprint()
        manifests = sorted(bg.OUTPUT.rglob("manifest.json"))
        self.assertGreaterEqual(len(manifests), 6)
        for path in manifests:
            data = json.loads(path.read_text())
            self.assertEqual(data["source_hash"], expected_source, f"Regenerate {path}")
            name = path.parent.parent.name
            source = bg.ROOT / "assets/levels" / (f"{name}.png" if path.parent.parent.parent.name == "custom" else f"maps/{name}.png")
            self.assertEqual(data["map_hash"], bg.file_hash(source), f"Map changed: {source}")
            self.check_gutters(path.parent, data)

    def check_gutters(self, directory, data):
        patches = data["patches"]
        images = [bg.load_rgba(directory / f"{i}.png") for i in range(len(patches))]
        for i, patch in enumerate(patches):
            for j in range(i + 1, len(patches)):
                other = patches[j]
                lo = np.maximum(patch["texture_min"], other["texture_min"])
                hi = np.minimum(patch["texture_max"], other["texture_max"])
                if np.any(hi <= lo):
                    continue
                crops = []
                for index, entry in [(i, patch), (j, other)]:
                    a, b = lo - entry["texture_min"], hi - entry["texture_min"]
                    crops.append(images[index][a[1]:b[1], a[0]:b[0]])
                np.testing.assert_array_equal(crops[0], crops[1], err_msg=f"{directory}: seam {i}/{j}")


def shape_key(mask):
    return mask.shape, mask.tobytes()


if __name__ == "__main__":
    unittest.main()
