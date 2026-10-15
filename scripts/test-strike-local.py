#!/usr/bin/env python3
"""Exercise the real original Strike GPU client and UDP authority on localhost."""
import json
from pathlib import Path
import re
import subprocess
ROOT=Path(__file__).resolve().parent.parent
out=ROOT/'target/recovery-net';out.mkdir(parents=True,exist_ok=True)
server=subprocess.Popen([str(ROOT/'target/release/ferravex-server'),'--bind','127.0.0.1:0','--ticks','320'],cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
try:
    line=server.stdout.readline()
    match=re.search(r'on (127\.0\.0\.1:\d+)',line)
    if not match:raise RuntimeError('local authority failed to start: '+line)
    run=subprocess.run([str(ROOT/'target/release/strike'),'--connect',match[1],'--play','--smoke-frames','180','--screenshot',str(out/'strike-network.png')],cwd=ROOT,capture_output=True,text=True,timeout=30)
    if run.returncode or 'mode=network connected=true' not in run.stdout or not re.search(r'server_tick=[1-9]\d*',run.stdout):
        raise RuntimeError('client did not receive authority snapshots: '+run.stdout+run.stderr)
    tail=server.communicate(timeout=15)[0]
    if server.returncode:raise RuntimeError('local authority failed: '+tail)
    report={'passed':True,'transport':'localhost UDP','client':run.stdout.strip(),'server':line+tail,'screenshot':str((out/'strike-network.png').relative_to(ROOT))}
    (out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
    print('PASS: original Strike client joined local authority and received advancing snapshots.\n'+run.stdout.strip())
finally:
    if server.poll() is None:server.terminate();server.wait(timeout=5)
