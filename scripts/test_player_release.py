"""Verify complete release coverage and all per-game upgrade destinations."""
import json
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parent.parent
class PlayerReleaseTests(unittest.TestCase):
 def test_release_contract(self):
  rows=json.loads((ROOT/'website/releases/player30.json').read_text())
  self.assertEqual([row['id'] for row in rows],list(range(1,31)))
  self.assertEqual(len({row['url'] for row in rows}),30)
  for row in rows:self.assertRegex(row['url'],r'^https://github.com/nearbycoder/ferravex/pull/[0-9]+$')
 def test_published_coverage(self):
  site=ROOT/'dist/site'
  release=(site/'features/index.html').read_text()
  for feature in json.loads((ROOT/'website/features.json').read_text()):self.assertIn(f'id="{feature["id"]}"',release)
  self.assertNotIn('/updates/',release)
  self.assertIn('url=/features/',(site/'updates/index.html').read_text())
  self.assertTrue((site/'docs/player-guide/index.html').is_file())
  for game in json.loads((ROOT/'examples/catalog.json').read_text()):
   page=(site/f"examples/{game['id']}/index.html").read_text()
   self.assertIn('/docs/player-guide/',page)
   self.assertIn('/features/',page)
   self.assertNotIn('/updates/',page)
  self.assertTrue((site/'play/player.js').is_file())
  self.assertTrue((site/'play/loader.js').is_file())
if __name__=='__main__':unittest.main()
