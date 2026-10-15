#!/usr/bin/env python3
"""Reject third-party packages in the first-party engine and reference game graph."""
import json,pathlib,subprocess,sys
root=pathlib.Path(__file__).resolve().parent.parent
host=next(line.split(': ',1)[1] for line in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if line.startswith('host: '))
metadata=json.loads(subprocess.check_output(['cargo','metadata','--format-version=1','--locked','--filter-platform',host],cwd=root))
packages={p['id']:p for p in metadata['packages']};nodes={n['id']:n for n in metadata['resolve']['nodes']}
roots={'ferravex','ferravex-engine','owned-lab','ferravex-fps','ferravex-core','owned-orbit'};pending=[p['id'] for p in packages.values() if p['name'] in roots];seen=set()
while pending:
    id=pending.pop()
    if id in seen:continue
    seen.add(id);pending.extend(nodes[id]['dependencies'])
external=[packages[id]['name'] for id in seen if packages[id]['source'] is not None]
if external:sys.exit('FAIL: external dependencies: '+', '.join(sorted(external)))
print('PASS: '+str(len(seen))+' workspace packages; zero third-party dependencies in '+', '.join(sorted(roots)))
