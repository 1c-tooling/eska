#!/usr/bin/python3
"""Mock process failures only; published analyzer validation has separate real-binary tests."""
import sys,json,time,os
from pathlib import Path
root=Path.cwd()
version=(root/'version').read_text() if (root/'version').exists() else '0.4.0'
if sys.argv[1]=='--version':
    print('bsl-analyzer '+version)
elif sys.argv[1]=='contract':
    print(json.dumps({'build_version':version,'contract_version':'3.5','cli':{'commands':[{'name':name,'args':[{'name':'config'}]} for name in ['check-config','analyze','lsp']]}}))
elif sys.argv[1]=='check-config':
    (root/'check-started').write_text(str(os.getpid()))
    if (root/'sleep').exists(): time.sleep(1)
    if (root/'mutate').exists():
        with (root/'eska.toml').open('a') as f: f.write('\n# concurrent change\n')
    if (root/'reject').exists(): print('rejected fixture',file=sys.stderr);sys.exit(3)
else: sys.exit(4)
