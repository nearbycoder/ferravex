#!/usr/bin/env python3
"""Build/stage every first-party browser game using Rust std and browser APIs."""
import argparse,hashlib,json,pathlib,re,shutil,subprocess
ROOT=pathlib.Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--profile',choices=['release','small','dev'],default='release');args=parser.parse_args()
subprocess.run(['python3','scripts/audit-owned.py'],cwd=ROOT,check=True)
games=json.loads((ROOT/'examples/catalog.json').read_text())
# Build optional authoring hosts separately: Cargo otherwise unifies their
# exported editing ABI into every Wasm game in a workspace-wide invocation.
for editing in (False,True):
    group=[game for game in games if bool(game.get('editable_scene'))==editing]
    if not group:continue
    command=['cargo','build','--offline','--locked','--lib','--target','wasm32-unknown-unknown','--profile',args.profile]
    for game in group:command.extend(['-p',game['id']])
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
shutil.copy2(ROOT/'web/runtime.js',output/'runtime.js')
shutil.copy2(ROOT/'web/lifecycle.js',output/'lifecycle.js')
for name in ('audio.js','audio-worklet.js','audio-ring.js'):shutil.copy2(ROOT/'web'/name,output/name)
shutil.copy2(ROOT/'web/input.js',output/'input.js')
shutil.copy2(ROOT/'web/storage.js',output/'storage.js')
shutil.copy2(ROOT/'web/input-overlay.js',output/'input-overlay.js')
shutil.copy2(ROOT/'web/retained.js',output/'retained.js')
shutil.copy2(ROOT/'web/effects.js',output/'effects.js')
# One shader implementation supplies both native GL and browser WebGL2.
parts=[]
for file,name,export in [('retained.rs','VERTEX_BODY','vertexBody'),('material.rs','FRAGMENT_BODY','fragmentBody'),('material.rs','DEFAULT_VERTEX_HOOK','defaultVertexHook'),('material.rs','DEFAULT_FRAGMENT_HOOK','defaultFragmentHook')]:
    source=(ROOT/'crates/ferravex-render/src'/file).read_text()
    match=re.search(r'pub const '+name+r':\s*&str\s*=\s*r#"(.*?)"#;',source,re.S)
    if not match:raise SystemExit('Missing shared shader template: '+name)
    parts.append('export const '+export+' = '+json.dumps(match.group(1))+';')
(output/'retained-shader.js').write_text('\n'.join(parts)+'\n')
parts=[]
for name,export in [('SPRITE_VERTEX_BODY','spriteVertexBody'),('SPRITE_FRAGMENT_BODY','spriteFragmentBody')]:
    source=(ROOT/'crates/ferravex-render/src/effects.rs').read_text()
    match=re.search(r'pub const '+name+r':\s*&str\s*=\s*r#"(.*?)"#;',source,re.S)
    if not match:raise SystemExit('Missing shared sprite shader: '+name)
    parts.append('export const '+export+' = '+json.dumps(match.group(1))+';')
(output/'effects-shader.js').write_text('\n'.join(parts)+'\n')
shutil.copy2(ROOT/'web/material-tools.js',output/'material-tools.js')
shutil.copy2(ROOT/'web/content-tools.js',output/'content-tools.js')
shutil.copy2(ROOT/'web/replay-tools.js',output/'replay-tools.js')
shutil.copy2(ROOT/'web/multiplayer-tools.js',output/'multiplayer-tools.js')
shutil.copy2(ROOT/'web/index.html',output/'index.html')
shutil.copy2(ROOT/'examples/catalog.json',output/'games.json')
manifest={str(p.relative_to(output)):{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in output.rglob('*') if p.is_file()}
(output/'build-info.json').write_text(json.dumps({'profile':args.profile,'external_packages':0,'files':manifest},indent=2)+'\n')
print(json.dumps(report,indent=2))
