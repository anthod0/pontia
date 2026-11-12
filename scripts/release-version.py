#!/usr/bin/env python3
"""Read and validate Pontia's product version for builds and releases."""

import argparse
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PLACEHOLDER = "0.0.0"
NPM_CLI_PLATFORMS = ("linux-x64", "linux-arm64", "darwin-x64", "darwin-arm64")
SEMVER = re.compile(
    r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-((?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
)


def product_version() -> str:
    version = (ROOT / "VERSION").read_text().strip()
    if not SEMVER.fullmatch(version):
        raise ValueError(f"VERSION is not valid semantic version: {version!r}")
    return version


def workspace_version() -> str:
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = manifest.get("workspace", {}).get("package", {}).get("version")
    if not isinstance(version, str):
        raise ValueError("Cargo.toml does not define [workspace.package].version")
    return version


def package_version(path: Path) -> str:
    value = json.loads(path.read_text()).get("version")
    if not isinstance(value, str):
        raise ValueError(f"{path.relative_to(ROOT)} does not define a string version")
    return value


def check() -> None:
    product_version()
    versions = {"Cargo.toml": workspace_version()}
    for relative in (
        "clients/pi/package.json",
        "clients/claude/package.json",
        "apps/cloud/package.json",
        "apps/dashboard/package.json",
        "apps/cron/package.json",
        "npm/cli/package.json",
        *(f"npm/cli-{platform}/package.json" for platform in NPM_CLI_PLATFORMS),
    ):
        versions[relative] = package_version(ROOT / relative)
    cli = json.loads((ROOT / "npm/cli/package.json").read_text())
    for name, version in cli["optionalDependencies"].items():
        versions[f"npm/cli/package.json optionalDependencies {name}"] = version
    invalid = {path: version for path, version in versions.items() if version != PLACEHOLDER}
    if invalid:
        details = ", ".join(f"{path}={version}" for path, version in invalid.items())
        raise ValueError(f"release manifests must use the {PLACEHOLDER} placeholder: {details}")


def materialize_npm(path: Path) -> None:
    package = json.loads(path.read_text())
    if package.get("version") != PLACEHOLDER:
        raise ValueError(f"npm package must use the {PLACEHOLDER} placeholder before release")
    version = product_version()
    package["version"] = version
    # @pontia/cli pins its platform packages to the same product version.
    for name, value in package.get("optionalDependencies", {}).items():
        if value != PLACEHOLDER:
            raise ValueError(f"optional dependency {name} must use the {PLACEHOLDER} placeholder before release")
        package["optionalDependencies"][name] = version
    path.write_text(json.dumps(package, indent=2, ensure_ascii=False) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("version", help="print the product version")
    subcommands.add_parser("check", help="validate the product version and manifest placeholders")
    materialize = subcommands.add_parser("materialize-npm", help="write the product version into a staged npm package")
    materialize.add_argument("package_json", type=Path)
    args = parser.parse_args()

    if args.command == "version":
        print(product_version())
    elif args.command == "check":
        check()
    else:
        materialize_npm(args.package_json)


if __name__ == "__main__":
    main()
