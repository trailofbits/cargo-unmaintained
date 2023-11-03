#! /bin/bash

set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "$0: expect no arguments" >&2
    exit 1
fi

sed -i 's/\<[0-9]\+ days ago\>/[..] days ago/' tests/cases/*.stdout
sed -i 's/\<latest: [^)]*/latest: [..]/' tests/cases/*.stdout
