#!/bin/sh
# usage: th.sh TAG "hrun args"   -> runs hrun.ps1 with args in Administrator's interactive session, prints result
tag="$1"; shift
printf "& 'C:\\\\nebulae-factory-hdr\\\\tools\\\\hrun.ps1' %s -Tag %s\n" "$*" "$tag" > /tmp/nebulae-hdr-diag-scratch/w_$tag.ps1
sed -i.bak 's/\\\\\\\\/\\\\/g' /tmp/nebulae-hdr-diag-scratch/w_$tag.ps1 2>/dev/null; rm -f /tmp/nebulae-hdr-diag-scratch/w_$tag.ps1.bak
cat /tmp/nebulae-hdr-diag-scratch/w_$tag.ps1 >&2
scp -o BatchMode=yes -q /tmp/nebulae-hdr-diag-scratch/w_$tag.ps1 "supermicro.local:C:/nebulae-factory-hdr/tools/w_$tag.ps1" 2>&1 | grep -v "post-quantum\|store now\|server may"
/tmp/nebulae-hdr-diag-scratch/rr.sh /tmp/nebulae-hdr-diag-scratch/task_any.ps1 -Script w_$tag.ps1 -Timeout ${TMO:-120}
