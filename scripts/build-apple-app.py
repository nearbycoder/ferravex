#!/usr/bin/env python3
"""Bundle a first-party native demo with the platform's installed Apple SDK.

Builds locally only: no upload, installation, CI or certificate provisioning.
--manifest-only emits reviewable bundle metadata without invoking an SDK/build.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shlex
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent

def manifest(game, platform_name, identifier):
    data = {'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': game['id'],
            'CFBundleIdentifier': identifier, 'CFBundleInfoDictionaryVersion': '6.0',
            'CFBundleName': game['title'], 'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': '0.1.0', 'CFBundleVersion': '1'}
    if platform_name == 'macos':
        data.update(NSHighResolutionCapable=True, LSMinimumSystemVersion='10.15')
    else:
        data.update(MinimumOSVersion='14.0', UIDeviceFamily=[1, 2],
                    UIRequiredDeviceCapabilities=['opengles-3'], UILaunchScreen={},
                    UISupportedInterfaceOrientations=['UIInterfaceOrientationPortrait',
                        'UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight'],
                    CADisableMinimumFrameDurationOnPhone=True, UIStatusBarHidden=True,
                    NSLocalNetworkUsageDescription='Connect to a Ferravex game server on your local network.',
                    UIApplicationSceneManifest={
                        'UIApplicationSupportsMultipleScenes': False,
                        'UISceneConfigurations': {'UIWindowSceneSessionRoleApplication': [{
                            'UISceneConfigurationName': 'Ferravex',
                            'UISceneClassName': 'UIWindowScene',
                            'UISceneDelegateClassName': 'FerravexSceneDelegate'}]}})
    return data

def main():
    games = {g['id']: g for g in json.loads((ROOT / 'examples/catalog.json').read_text())
             if g['id'] not in ('owned-lab', 'owned-orbit')}
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('game', choices=games)
    parser.add_argument('--platform', choices=('macos', 'ios', 'ios-simulator'), default='macos')
    parser.add_argument('--profile', choices=('release', 'small'), default='release')
    parser.add_argument('--identifier')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--manifest-only', action='store_true')
    parser.add_argument('--identity', help='Existing local codesign identity; no certificate is created')
    parser.add_argument('--provision', type=Path, help='Existing mobile provisioning profile for a device bundle')
    args = parser.parse_args()
    identifier = args.identifier or f'com.nearbycoder.ferravex.{args.game}'
    if not re.fullmatch(r'[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+', identifier):
        parser.error('identifier must be a reverse-DNS bundle identifier')
    bundle = (args.output or ROOT / 'target/apple-apps' / args.platform / f'{args.game}.app').resolve()
    contents = bundle / 'Contents' if args.platform == 'macos' else bundle
    contents.mkdir(parents=True, exist_ok=True)
    with (contents / 'Info.plist').open('wb') as output:
        plistlib.dump(manifest(games[args.game], args.platform, identifier), output)
    if args.manifest_only:
        print(contents / 'Info.plist')
        return
    if platform.system() != 'Darwin':
        parser.error('native Apple app builds require macOS and the installed Apple SDK; the manifest was preserved')
    arm = platform.machine() == 'arm64'
    target = {'macos': 'aarch64-apple-darwin' if arm else 'x86_64-apple-darwin',
              'ios': 'aarch64-apple-ios',
              'ios-simulator': 'aarch64-apple-ios-sim' if arm else 'x86_64-apple-ios'}[args.platform]
    sdk = {'macos': 'macosx', 'ios': 'iphoneos', 'ios-simulator': 'iphonesimulator'}[args.platform]
    sdk_path = subprocess.check_output(['xcrun', '--sdk', sdk, '--show-sdk-path'], text=True).strip()
    linker = ROOT / 'target/apple-apps' / f'link-{target}.sh'
    linker.parent.mkdir(parents=True, exist_ok=True)
    linker.write_text(f'#!/bin/sh\nexec xcrun --sdk {shlex.quote(sdk)} clang "$@"\n')
    linker.chmod(0o700)
    env = {**os.environ, 'SDKROOT': sdk_path,
           'CARGO_TARGET_' + target.replace('-', '_').upper() + '_LINKER': str(linker)}
    env['MACOSX_DEPLOYMENT_TARGET' if args.platform == 'macos' else 'IPHONEOS_DEPLOYMENT_TARGET'] = '10.15' if args.platform == 'macos' else '14.0'
    subprocess.run(['cargo', 'build', '--offline', '--locked', '--target', target,
                    '--profile', args.profile, '-p', args.game, '--bin', args.game], cwd=ROOT, env=env, check=True)
    executable = contents / 'MacOS' / args.game if args.platform == 'macos' else bundle / args.game
    executable.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ROOT / 'target' / target / args.profile / args.game, executable)
    if args.provision:
        if args.platform != 'ios':
            parser.error('--provision applies only to physical iOS bundles')
        shutil.copy2(args.provision, bundle / 'embedded.mobileprovision')
    if args.identity:
        subprocess.run(['codesign', '--force', '--timestamp=none', '--sign', args.identity, str(bundle)], check=True)
    print(bundle)

if __name__ == '__main__':
    main()
