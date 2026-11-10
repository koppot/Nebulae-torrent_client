#!/bin/sh
# usage: rr.sh script.ps1 [args...]  (args passed as -File style via base64 wrapper)
f="$1"; shift
scp -o BatchMode=yes -q "$f" 'supermicro.local:C:/nebulae-factory-hdr/tools/'"$(basename $f)" 2>&1 | grep -v "post-quantum\|store now\|server may"
ssh -o BatchMode=yes -o ConnectTimeout=15 supermicro.local "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File C:\\nebulae-factory-hdr\\tools\\$(basename $f) $*" 2>&1 | grep -v "post-quantum\|store now\|server may\|CLIXML\|<Objs\|</Objs"
