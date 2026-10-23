#!/usr/bin/env python3
"""Local Ferravex commands, using only Python's standard library.

Examples (also available in --help)::

    python3 scripts/ferravex.py list --json
    python3 scripts/ferravex.py run neon-swarm
    python3 scripts/ferravex.py run owned-orbit -- --autoplay --frames 120
    python3 scripts/ferravex.py build-web small --dry-run
    python3 scripts/ferravex.py test -p ferravex-core -- --nocapture
    python3 scripts/ferravex.py check --dry-run
    python3 scripts/ferravex.py check -p ferravex-core --result-json
    python3 scripts/ferravex.py --result-json test -p ferravex-core --dry-run
    python3 scripts/ferravex.py doctor --json
    python3 scripts/ferravex.py new --help

Cargo commands use --offline --locked and at most two jobs. Test/check select
the workspace and all features by default; -p narrows the package selection.
Run defaults to the release profile. Courtyard (owned-lab) and Orbit Breaker
(owned-orbit) keep their native headless behavior; this command does not change
their hosts. Build-web stages the existing local web build, without serving it.

--dry-run may appear before or after the subcommand and prints a JSON object
with argv, cwd and env overrides without executing anything. List --json emits
the catalog array. Arguments after -- belong to the game or test harness.
Doctor/new forward their arguments (including --help) to their respective
scripts; place --dry-run before -- to preview delegation. New runs in the
caller's directory so relative scaffold destinations retain their meaning.

--result-json opts into a versioned outcome for every command, including usage
errors and help. It may appear before or after the command, before --. Stdout
contains exactly one JSON object; child stdout streams to stderr alongside child
stderr, without buffering logs in memory. Existing --json flags retain their
meaning (catalog output or a forwarded helper flag).

Outcome schema_version 1 always has these fields:
  command: selected subcommand, or null if parsing has not selected one.
  status: ok, dry_run, help, or error; dry_run never claims execution succeeded.
  exit_code: the CLI exit status; child_returncode: raw child status or null.
  dry_run: whether the wrapper's --dry-run was requested.
  plan: {argv, cwd, env} after wrapper validation, otherwise null.
        env contains only overrides, never the inherited process environment.
  data: catalog array for list, {help: text} for help, otherwise null.
  error: null, or {code, message, hint, details}. Code is machine-readable;
         message/hint are for humans and may change. Details are code-specific.
Consumers should check schema_version and error.code and allow extra fields.

Error codes: invalid_arguments, unknown_arguments, unknown_package,
unknown_game, invalid_catalog, invalid_workspace, missing_manifest,
missing_helper, executable_not_found, permission_denied, launch_failed,
interrupted, child_failed, child_signaled. Argument/input errors exit with 2.
Child statuses are preserved, including 128+signal on signal termination.
Missing executables return 127, permission errors 126, other launch errors 1,
and Ctrl-C 130. No child output is embedded in the outcome.
"""

import argparse
import difflib
import glob
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parent.parent
_NAME = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9_-]*\Z")
_HELPERS = {"doctor": "doctor.py", "new": "new-game.py"}


class _CommandError(ValueError):
    def __init__(self, code, message, hint, *, details=None, parser=None):
        super().__init__(message)
        self.report = {"code": code, "message": message, "hint": hint, "details": details or {}}
        self.parser = parser


class _HelpRequested(Exception):
    def __init__(self, text):
        self.text = text


class _Parser(argparse.ArgumentParser):
    def __init__(self, *args, outcome=None, command_name=None, **kwargs):
        super().__init__(*args, **kwargs)
        self.outcome = outcome
        self.command_name = command_name

    def parse_known_args(self, args=None, namespace=None):
        if self.outcome is not None and self.command_name is not None:
            self.outcome["command"] = self.command_name
        return super().parse_known_args(args, namespace)

    def error(self, message):
        hint = f"Run '{self.prog} --help' for accepted arguments and values."
        if self.command_name in ("run", "test"):
            hint += " Put game or test-harness arguments after --."
        raise _CommandError(
            "invalid_arguments", message, hint,
            details={"usage": self.format_usage().strip()}, parser=self,
        )

    def print_help(self, file=None):
        if self.outcome is not None:
            raise _HelpRequested(self.format_help())
        super().print_help(file)


def _finish(outcome, status, exit_code, *, data=None, error=None):
    if outcome is not None:
        outcome.update(status=status, exit_code=exit_code, data=data, error=error)
        print(json.dumps(outcome, indent=2, ensure_ascii=True))
    return exit_code


def _unknown_name(kind, value, choices, *, suggestions=None):
    choices = sorted(choices)
    if suggestions is None:
        suggestions = difflib.get_close_matches(value, choices, n=3, cutoff=0.6)
    hint = "Did you mean " + ", ".join(repr(name) for name in suggestions) + "? " if suggestions else ""
    if kind == "game":
        hint += "Run 'ferravex.py list' for catalog ids (host aliases are not package ids)."
    else:
        hint += "Use --package NAME with a workspace package from Cargo.toml."
    hint += " Available " + kind + "s: " + (", ".join(choices) or "(none)") + "."
    return _CommandError(
        "unknown_" + kind, f"unknown {kind} {value!r}", hint,
        details={"value": value, "choices": choices, "suggestions": suggestions},
    )


def _manifest(path):
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error


def load_packages(root: Path = ROOT) -> list[str]:
    """Return sorted, declared workspace package names without launching Cargo.

    Read package.name from member manifests, not directory names or the game
    catalog. Supports member globs, excludes and a package at the workspace root.
    Ferravex declares all its packages explicitly; this is not a Cargo dependency
    resolver and does not discover implicit path-dependency members. Invalid or
    missing manifests raise ValueError with file context. Requires Python 3.11+.

    For example, validate a targeted command's package before launching a build::

        assert "ferravex-core" in load_packages()
    """
    root = root.resolve()
    path = root / "Cargo.toml"
    manifest = _manifest(path)
    workspace = manifest.get("workspace")
    if not isinstance(workspace, dict):
        raise ValueError(f"{path}: expected a workspace table")
    paths = {}
    for field in ("members", "exclude"):
        patterns = workspace.get(field, [])
        if not isinstance(patterns, list) or any(
            not isinstance(pattern, str) or not pattern or Path(pattern).is_absolute()
            or ".." in Path(pattern).parts for pattern in patterns
        ):
            raise ValueError(f"{path}: workspace.{field} must contain relative paths inside the checkout")
        paths[field] = set()
        for pattern in patterns:
            matches = {member.resolve() for member in root.glob(pattern) if member.is_dir()}
            if field == "members" and not matches and not glob.has_magic(pattern):
                raise ValueError(f"{path}: missing workspace member {pattern!r}; restore its Cargo.toml")
            paths[field].update(matches)
    members = paths["members"] - paths["exclude"]
    if "package" in manifest:
        members.add(root)
    packages = {}
    for member in sorted(members):
        member_path = member / "Cargo.toml"
        if not member.is_relative_to(root):
            raise ValueError(f"{path}: workspace member {member} is outside the checkout")
        package = _manifest(member_path).get("package")
        name = package.get("name") if isinstance(package, dict) else None
        if not isinstance(name, str) or not _NAME.fullmatch(name):
            raise ValueError(f"{member_path}: expected a simple package.name")
        if name in packages:
            raise ValueError(f"{member_path}: duplicate package {name!r}; also declared in {packages[name]}")
        packages[name] = member_path
    return sorted(packages)


def load_games(root: Path = ROOT) -> list[dict]:
    """Read and validate catalog identities; raise ValueError with file context.

    The returned records retain all catalog metadata, in catalog order::

        ids = [game["id"] for game in load_games()]

    No subprocesses are used, so listing games also works without Cargo.
    Non-finite JSON numbers are rejected, including overflow such as 1e999.
    """
    path = root / "examples" / "catalog.json"
    try:
        games = json.loads(path.read_text(encoding="utf-8"),
                           parse_float=_finite_float, parse_constant=_finite_float)
    except (OSError, UnicodeError, ValueError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error
    if not isinstance(games, list):
        raise ValueError(f"{path}: expected an array of games")
    seen = set()
    for index, game in enumerate(games):
        location = f"{path}: entry {index + 1}"
        if not isinstance(game, dict):
            raise ValueError(f"{location}: expected a game object")
        for field in ("id", "host"):
            value = game.get(field)
            if not isinstance(value, str) or not _NAME.fullmatch(value):
                raise ValueError(f"{location}: invalid {field}; expected a simple name")
        if not isinstance(game.get("title"), str) or not game["title"].strip():
            raise ValueError(f"{location}: title must be a nonempty string")
        if game["id"] in seen:
            raise ValueError(f"{location}: duplicate game id {game['id']!r}")
        seen.add(game["id"])
    return games


def _finite_float(value):
    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"non-finite JSON number {value!r}")
    return number


def _package(value: str) -> str:
    if not _NAME.fullmatch(value):
        raise argparse.ArgumentTypeError(
            f"invalid package {value!r}; expected a Cargo package name, e.g. ferravex-core",
        )
    return value


def _parser(outcome=None) -> argparse.ArgumentParser:
    common = argparse.ArgumentParser(add_help=False)
    # Suppressed defaults keep a subparser from clearing a flag given globally.
    common.add_argument(
        "--dry-run", action="store_true", default=argparse.SUPPRESS,
        help="print the command as JSON without running it",
    )
    common.add_argument(
        "--result-json", action="store_true", default=argparse.SUPPRESS,
        help="emit a versioned command outcome on stdout; stream child output to stderr",
    )
    parser = _Parser(
        prog="ferravex.py", outcome=outcome,
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter,
        parents=[common], allow_abbrev=False,
    )
    sub = parser.add_subparsers(dest="command", required=True)

    def command(name, help_text, **kwargs):
        return sub.add_parser(
            name, help=help_text, description=help_text, parents=[common],
            allow_abbrev=False, outcome=outcome, command_name=name, **kwargs,
        )

    listing = command("list", "List the demos from examples/catalog.json.")
    listing.add_argument("--json", action="store_true", help="emit the catalog as JSON")

    run = command("run", "Run a catalog game natively; pass game arguments after --.")
    run.add_argument("game", metavar="GAME", help="catalog id (see list)")
    run.add_argument("--profile", choices=("dev", "release", "small"), default="release")
    run.add_argument("--jobs", type=int, choices=(1, 2), default=2)

    web = command("build-web", "Build/stage all browser demos with the existing web builder.")
    web.add_argument("profile", nargs="?", choices=("release", "small"), default="release")
    web.add_argument("--jobs", type=int, choices=(1, 2), default=2)

    for name in ("test", "check"):
        cargo = command(name, f"Cargo {name} with all features; defaults to the workspace.")
        cargo.add_argument("-p", "--package", type=_package, action="append", default=[])
        cargo.add_argument("--profile", choices=("dev", "release", "small"), default="dev")
        cargo.add_argument("--jobs", type=int, choices=(1, 2), default=2)
        if name == "test":
            cargo.add_argument("filter", nargs="?", help="optional Rust test name filter")
            cargo.add_argument("--lib", action="store_true", help="run library tests only")

    for name, script in _HELPERS.items():
        command(name, f"Forward arguments to scripts/{script} (including --help).", add_help=False)
    return parser


def _script(root: Path, name: str) -> list[str]:
    path = root / "scripts" / name
    if not path.is_file():
        raise _CommandError(
            "missing_helper", f"missing helper {path}",
            "Restore that script in the checkout before retrying.", details={"path": str(path)},
        )
    return [sys.executable, "-B", str(path)]


def main(argv: list[str] | None = None, *, root: Path = ROOT) -> int:
    """Route a command and return its exit status (argument errors exit with 2).

    A caller can inspect a command without launching any build::

        status = main(["check", "--dry-run"])
        status = main(["--result-json", "test", "-p", "ferravex-core", "--dry-run"])

    Normal child statuses are preserved; signal termination maps to 128+signal.
    Missing executables return 127, launch permission errors 126 and Ctrl-C 130.
    With --result-json, argument errors return 2 and help returns 0, each with
    one schema_version 1 object on stdout. Without it, argparse errors/help
    retain their SystemExit behavior. See the module docstring for the schema.
    """
    arguments = list(sys.argv[1:] if argv is None else argv)
    tail = []
    has_separator = "--" in arguments
    if has_separator:
        separator = arguments.index("--")
        arguments, tail = arguments[:separator], arguments[separator + 1:]
    # Detect only the wrapper's exact flag, even when argument parsing fails.
    # Flags beyond -- must remain literal child arguments.
    outcome = None
    if "--result-json" in arguments:
        outcome = {
            "schema_version": 1, "command": None, "status": None,
            "exit_code": None, "child_returncode": None,
            "dry_run": "--dry-run" in arguments, "plan": None, "data": None, "error": None,
        }
    parser = _parser(outcome)
    try:
        args, unknown = parser.parse_known_args(arguments)
        if args.command not in _HELPERS:
            if unknown:
                hint = f"Run 'ferravex.py {args.command} --help' for accepted arguments."
                if args.command in ("run", "test"):
                    hint += " Put game or test-harness arguments after --."
                raise _CommandError(
                    "unknown_arguments", "unrecognized arguments: " + " ".join(unknown),
                    hint, details={"arguments": unknown},
                )
            if has_separator and args.command not in ("run", "test"):
                parser.error("arguments after -- are supported only for run, test, doctor and new")
        dry_run = getattr(args, "dry_run", False)
        try:
            root = root.resolve()
            cwd = Path.cwd() if args.command == "new" else root
        except OSError as error:
            raise _CommandError(
                "invalid_workspace", str(error),
                "Ensure the checkout and caller's working directory exist and are accessible.",
            ) from error
        if args.command == "list":
            games = _games(root)
            if outcome is not None:
                return _finish(outcome, "dry_run" if dry_run else "ok", 0, data=games)
            if args.json or dry_run:
                print(json.dumps(games, indent=2, ensure_ascii=False))
            elif not games:
                print("No games in examples/catalog.json.")
            else:
                width = max(len(game["id"]) for game in games)
                for game in games:
                    print(f"{game['id']:<{width}}  {game['title']}  (/{game['host']}/)")
            return 0

        if args.command in _HELPERS:
            command = _script(root, _HELPERS[args.command]) + unknown
            if has_separator:
                command += ["--", *tail]
        elif args.command == "build-web":
            command = _script(root, "build-web.py") + [args.profile]
        else:
            command = ["cargo", args.command, "--offline", "--locked", "--jobs", str(args.jobs)]
            if args.command == "run":
                games = _games(root)
                if args.game not in {game["id"] for game in games}:
                    aliases = [game["id"] for game in games if game["host"] == args.game]
                    raise _unknown_name("game", args.game, [game["id"] for game in games],
                                        suggestions=aliases or None)
                manifest = root / "examples" / args.game / "Cargo.toml"
                if not manifest.is_file():
                    raise _CommandError(
                        "missing_manifest", f"game {args.game!r} is missing {manifest}",
                        "Restore the game's Cargo.toml or select another id from 'ferravex.py list'.",
                        details={"path": str(manifest)},
                    )
                command += ["--package", args.game]
            else:
                if args.package:
                    try:
                        packages = load_packages(root)
                    except (OSError, ValueError) as error:
                        raise _CommandError(
                            "invalid_workspace", str(error),
                            "Check Cargo.toml workspace.members/exclude and the member package manifests.",
                        ) from error
                    for package in args.package:
                        if package not in packages:
                            raise _unknown_name("package", package, packages)
                        command += ["--package", package]
                else:
                    command += ["--workspace"]
                command += ["--all-features"]
                if args.command == "check":
                    command += ["--all-targets"]
                elif args.lib:
                    command += ["--lib"]
            command += ["--profile", args.profile]
            if args.command == "test" and args.filter is not None:
                command.append(args.filter)
            if has_separator:
                command += ["--", *tail]

        env = {"CARGO_NET_OFFLINE": "true", "CARGO_BUILD_JOBS": str(getattr(args, "jobs", 2))}
        plan = {"argv": command, "cwd": str(cwd), "env": env}
        if outcome is not None:
            outcome["plan"] = plan
        if dry_run:
            if outcome is None:
                print(json.dumps(plan, indent=2))
            return _finish(outcome, "dry_run", 0)
        try:
            # In result mode, keep stdout exclusively for the outcome. Inherit
            # stderr directly: no PIPE, unbounded capture, decoding or log files.
            streams = {"stdout": sys.stderr} if outcome is not None else {}
            result = subprocess.run(
                command, cwd=cwd, env={**os.environ, **env}, shell=False, check=False, **streams,
            )
        except OSError as error:
            if isinstance(error, FileNotFoundError):
                code, status, action = "executable_not_found", 127, "find"
                hint = f"Ensure {command[0]!r} is installed/on PATH and the plan's cwd still exists."
            elif isinstance(error, PermissionError):
                code, status, action = "permission_denied", 126, "execute"
                hint = "Check executable permissions and access to the plan's cwd."
            else:
                code, status, action = "launch_failed", 1, "launch"
                hint = "Check the executable, working directory and operating-system error."
            report = _CommandError(
                code, f"cannot {action} {command[0]!r}: {error}", hint,
                details={"errno": error.errno},
            ).report
            if outcome is None:
                print(f"ferravex: {report['message']}", file=sys.stderr)
            return _finish(outcome, "error", status, error=report)
        if outcome is not None:
            outcome["child_returncode"] = result.returncode
        status = result.returncode if result.returncode >= 0 else 128 - result.returncode
        report = None
        if status:
            report = _CommandError(
                "child_signaled" if result.returncode < 0 else "child_failed",
                f"Child command exited with status {result.returncode}.",
                "Inspect child output on stderr and the argv/cwd/env in plan before retrying.",
            ).report
        return _finish(outcome, "error" if status else "ok", status, error=report)
    except _HelpRequested as help_request:
        return _finish(outcome, "help", 0, data={"help": help_request.text})
    except _CommandError as error:
        if outcome is not None:
            return _finish(outcome, "error", 2, error=error.report)
        error_parser = error.parser or parser
        error_parser.print_usage(sys.stderr)
        error_parser.exit(2, f"{error_parser.prog}: error: {error}\nHint: {error.report['hint']}\n")
    except KeyboardInterrupt:
        return _finish(outcome, "error", 130, error=_CommandError(
            "interrupted", "Command interrupted.", "Inspect any child output before retrying.",
        ).report)


def _games(root):
    try:
        return load_games(root)
    except ValueError as error:
        raise _CommandError(
            "invalid_catalog", str(error),
            "Restore or correct examples/catalog.json; each game needs a unique id, host and title.",
        ) from error


if __name__ == "__main__":
    raise SystemExit(main())
