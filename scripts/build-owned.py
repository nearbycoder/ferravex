#!/usr/bin/env python3
"""Build/stage every first-party browser game using Rust std and browser APIs."""
import argparse,hashlib,json,pathlib,shutil,subprocess
ROOT=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--profile',choices=['release','small','dev'],default='release');args=parser.parse_args()
subprocess.run(['python3','scripts/audit-owned.py'],cwd=ROOT,check=True)
games=json.loads((ROOT/'examples/catalog.json').read_text())
command=['cargo','build','--offline','--locked','--lib','--target','wasm32-unknown-unknown','--profile',args.profile]
for game in games:command.extend(['-p',game['id']])
subprocess.run(command,cwd=ROOT,check=True)
folder='debug' if args.profile=='dev' else args.profile
report={}
output=ROOT/'dist/web'
# Always stage into a fresh tree: retired modules/loaders must never survive a rebuild.
shutil.rmtree(output,ignore_errors=True)
for game in games:
    package=game['id'].replace('-','_');folder_name=game['host']
    out=output/folder_name;out.mkdir(parents=True,exist_ok=True)
    for path in (ROOT/'web'/folder_name).iterdir():
        if path.is_file():shutil.copy2(path,out/path.name)
    wasm=ROOT/'target/wasm32-unknown-unknown'/folder/(package+'.wasm');shutil.copy2(wasm,out/wasm.name)
    report[package]={'profile':args.profile,'wasm_bytes':wasm.stat().st_size,'files':{p.name:p.stat().st_size for p in out.iterdir() if p.is_file()},'third_party_runtime_packages':0}
shutil.copy2(ROOT/'web/index.html',output/'index.html')
shutil.copy2(ROOT/'examples/catalog.json',output/'games.json')
manifest={str(p.relative_to(output)):{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in output.rglob('*') if p.is_file()}
(output/'build-info.json').write_text(json.dumps({'profile':args.profile,'external_packages':0,'files':manifest},indent=2)+'\n')
print(json.dumps(report,indent=2))
