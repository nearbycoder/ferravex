"""Regression checks for the narrowly scoped Macroquad loader compatibility fix."""

import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import unittest

SPEC = importlib.util.spec_from_file_location("stage_web", Path(__file__).with_name("stage-web.py"))
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)

PLUGIN = (b'"use strict";(function(){function i(){}register_plugin=function(e){e.env.ws_connect=r},'
          b'miniquad_add_plugin({register_plugin,on_init:i,version:1,name:"quad_net"});function r(){}}())')


class LoaderPatchTests(unittest.TestCase):
    def test_preserves_plugin_registration_and_all_surrounding_bytes(self):
        original = PLUGIN
        patched = STAGE.patch_macroquad_loader(original)
        self.assertEqual(patched.replace(b"var register_plugin;", b"", 1), original)
        self.assertEqual(len(patched), len(original) + len(b"var register_plugin;"))
        self.assertIn(b'miniquad_add_plugin({register_plugin,on_init:i,version:1,name:"quad_net"})', patched)

    def test_changed_or_already_patched_upstream_fails_closed(self):
        for source in (b"new upstream loader", b"function i(){}var register_plugin;register_plugin=function"):
            with self.subTest(source=source), self.assertRaises(ValueError):
                STAGE.patch_macroquad_loader(source)

    def test_ambiguous_upstream_fails_closed(self):
        with self.assertRaises(ValueError):
            STAGE.patch_macroquad_loader(b"function i(){}register_plugin=function" * 2)

    @unittest.skipUnless(shutil.which("node"), "Node is optional; needed to execute the JS regression fixture")
    def test_strict_mode_registration_executes_and_stays_local(self):
        script = r'''
const vm = require('node:vm');
const input = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
let failed = false;
try { vm.runInNewContext(input.original, { miniquad_add_plugin() {} }); }
catch (error) { if (error.name !== 'ReferenceError') throw error; failed = true; }
if (!failed) throw new Error('Fixture should reproduce the upstream error');
const plugins = [];
const context = { miniquad_add_plugin(plugin) { plugins.push(plugin); } };
vm.runInNewContext(input.patched, context);
if (plugins.length !== 1 || plugins[0].name !== 'quad_net') throw new Error('Registration changed');
const imports = { env: {} };
plugins[0].register_plugin(imports);
if (typeof imports.env.ws_connect !== 'function') throw new Error('Plugin function lost');
if ('register_plugin' in context) throw new Error('Registration leaked into global scope');
'''
        subprocess.run(["node", "-e", script], input=json.dumps({
            "original": PLUGIN.decode(), "patched": STAGE.patch_macroquad_loader(PLUGIN).decode(),
        }), text=True, check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
