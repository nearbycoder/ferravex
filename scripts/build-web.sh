#!/bin/sh
# Build all catalogued browser examples and stage a self-contained static directory.
set -eu

profile=${1:-release}
case "$profile" in
    release|small) ;;
    *) echo "Usage: $0 [release|small]" >&2; exit 2 ;;
esac
if [ "$#" -gt 1 ]; then
    echo "Usage: $0 [release|small]" >&2
    exit 2
fi

repo_dir=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
cd "$repo_dir"
command -v python3 >/dev/null 2>&1 || { echo "Python 3 is required." >&2; exit 1; }
python3 scripts/build-web.py "$profile"
