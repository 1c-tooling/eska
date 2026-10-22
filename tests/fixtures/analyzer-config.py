#!/usr/bin/python3
"""Mock process failures only; published analyzer validation has separate real-binary tests."""
import sys,json,time,os
from pathlib import Path
root=Path.cwd()
if sys.argv[1]=='--version':
    print('bsl-analyzer '+((root/'version').read_text() if (root/'version').exists() else '0.3.2'))
elif sys.argv[1]=='contract':
    print(json.dumps({'build_version':'0.3.2','contract_version':'3.4','cli':{'commands':[{'name':name,'args':[{'name':'config'}]} for name in ['check-config','analyze','lsp']]}}))
elif sys.argv[1]=='check-config':
    (root/'check-started').write_text(str(os.getpid()))
    if (root/'sleep').exists(): time.sleep(1)
    if (root/'mutate').exists():
        with (root/'eska.toml').open('a') as f: f.write('\n# concurrent change\n')
    if (root/'reject').exists(): print('rejected fixture',file=sys.stderr);sys.exit(3)
else: sys.exit(4)
