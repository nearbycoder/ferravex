#!/usr/bin/env python3
"""Build/stage the original engine example. Only Rust std and browser APIs are required."""
import argparse,json,pathlib,shutil,subprocess
ROOT=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--profile',choices=['release','small','dev'],default='release');args=parser.parse_args()
subprocess.run(['cargo','build','--locked','-p','owned-lab','-p','owned-orbit','--lib','--target','wasm32-unknown-unknown','--profile',args.profile],cwd=ROOT,check=True)
folder='debug' if args.profile=='dev' else args.profile
report={}
for package,folder_name in [('owned_lab','owned'),('owned_orbit','owned-orbit')]:
    out=ROOT/'dist/web'/folder_name;out.mkdir(parents=True,exist_ok=True)
    for path in (ROOT/'web'/folder_name).iterdir():
        if path.is_file():shutil.copy2(path,out/path.name)
    wasm=ROOT/'target/wasm32-unknown-unknown'/folder/(package+'.wasm');shutil.copy2(wasm,out/wasm.name)
    report[package]={'profile':args.profile,'wasm_bytes':wasm.stat().st_size,'files':{p.name:p.stat().st_size for p in out.iterdir() if p.is_file()},'third_party_runtime_packages':0}
landing=ROOT/'dist/web/index.html'
if not landing.exists():
    landing.write_text('<!doctype html><html lang="en"><meta charset="utf-8"><meta http-equiv="refresh" content="0; url=owned/"><title>Ferravex</title><a href="owned/">Courtyard</a> · <a href="owned-orbit/">Orbit Breaker</a></html>')
print(json.dumps(report,indent=2))
