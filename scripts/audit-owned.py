#!/usr/bin/env python3
"""Reject every non-workspace Cargo package, across all features and platforms."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def validate_metadata(metadata, root=ROOT):
    members = set(metadata['workspace_members'])
    if not members:
        raise ValueError('workspace is empty')
    for package in metadata['packages']:
        name = package['name']
        if package.get('source') is not None or package['id'] not in members:
            raise ValueError(f'non-workspace dependency: {name}')
        if not Path(package['manifest_path']).resolve().is_relative_to(root.resolve()):
            raise ValueError(f'package outside the repository: {name}')
    if {p['id'] for p in metadata['packages']} != members:
        raise ValueError('incomplete workspace metadata')
    return len(members)

def main():
    # Deliberately omit --filter-platform: target-specific and optional packages
    # must be checked even when this host does not compile them.
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--format-version=1', '--all-features', '--locked', '--offline',
    ], cwd=ROOT))
    try:
        count = validate_metadata(metadata)
    except ValueError as error:
        raise SystemExit('FAIL: ' + str(error)) from error
    print(f'PASS: entire workspace ({count} packages), all features and targets; zero external Cargo packages')

if __name__ == '__main__':
    main()
