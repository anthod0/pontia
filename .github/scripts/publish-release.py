#!/usr/bin/env python3
"""Prepare signed releases and publish verified objects to R2 (CI only)."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ORIGIN = "https://get.pontia.dev"
ROOT = Path(__file__).resolve().parents[2]
TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu")


def run(*args):
    return subprocess.check_output(args)


def public_key():
    key = os.environ["RELEASE_PUBLIC_KEY"].strip()
    # Parse before embedding; only a PEM public key may enter shell source.
    with tempfile.TemporaryDirectory(prefix="pontia-public-key-") as directory:
        path = Path(directory) / "public.pem"
        path.write_text(key)
        # Ed25519 SPKI consists of its fixed algorithm header and 32-byte public key.
        der = run("openssl", "pkey", "-pubin", "-in", str(path), "-pubout", "-outform", "DER")
        if len(der) != 44 or not der.startswith(bytes.fromhex("302a300506032b6570032100")):
            raise ValueError("RELEASE_PUBLIC_KEY must be an Ed25519 public key")
        canonical = run("openssl", "pkey", "-pubin", "-in", str(path), "-pubout").decode().strip()
    return canonical


def prepare_installers(destination):
    destination.mkdir(parents=True, exist_ok=True)
    key = public_key()
    for name in ("install.sh", "install-edge.sh"):
        source = (ROOT / "scripts" / name).read_text()
        assert source.count("@PONTIA_RELEASE_PUBLIC_KEY@") == 1
        (destination / name).write_text(source.replace("@PONTIA_RELEASE_PUBLIC_KEY@", key))


def version_order(version):
    match = re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?", version)
    if not match:
        raise ValueError("Invalid semantic release version")
    major, minor, patch, prerelease = match.groups()
    identifiers = tuple((0, int(item)) if item.isdigit() else (1, item) for item in prerelease.split(".")) if prerelease else ()
    return (int(major), int(minor), int(patch), prerelease is None, identifiers)


def prepare_release(directory, version):
    version_order(version)
    expected = {f"{binary}-{target}.tar.gz" for binary in ("pontia", "pontiad", "pontia-edge") for target in TARGETS}
    expected.add("pontia-dashboard.tar.gz")
    if {p.name for p in directory.glob("*.tar.gz")} != expected:
        raise ValueError("Release artifact set does not match supported platforms")
    artifacts = {}
    checksums = []
    for name in sorted(expected):
        data = (directory / name).read_bytes()
        checksum = hashlib.sha256(data).hexdigest()
        artifacts[name] = {"url": f"{ORIGIN}/releases/{version}/{name}", "sha256": checksum, "size": len(data)}
        checksums.append(f"{checksum}  {name}\n")
    (directory / "SHA256SUMS").write_text("".join(checksums))
    payload = json.dumps({"schema_version": 1, "version": version, "artifacts": artifacts}, sort_keys=True, separators=(",", ":")).encode()
    with tempfile.TemporaryDirectory(prefix="pontia-release-sign-") as temporary:
        root = Path(temporary)
        key = root / "private.pem"
        key.touch(mode=0o600)
        key.write_text(os.environ["RELEASE_SIGNING_KEY"])
        public = public_key()
        derived_public = run("openssl", "pkey", "-in", str(key), "-pubout").decode().strip()
        if derived_public != public:
            raise ValueError("RELEASE_SIGNING_KEY does not match RELEASE_PUBLIC_KEY")
        (root / "public.pem").write_text(public)
        (root / "manifest").write_bytes(payload)
        signature = run("openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(key), "-in", str(root / "manifest"))
        (root / "signature").write_bytes(signature)
        run("openssl", "pkeyutl", "-verify", "-rawin", "-pubin", "-inkey", str(root / "public.pem"), "-sigfile", str(root / "signature"), "-in", str(root / "manifest"))
    envelope = {"manifest": base64.b64encode(payload).decode(), "signature": base64.b64encode(signature).decode()}
    (directory / "manifest.json").write_text(json.dumps(envelope) + "\n")


def aws(*args):
    return run("aws", "--endpoint-url", os.environ["R2_ENDPOINT"], "s3api", *args)


def upload(path, key, immutable=False):
    args = ["put-object", "--bucket", os.environ["R2_BUCKET"], "--key", key, "--body", str(path), "--cache-control", "public, max-age=31536000, immutable" if immutable else "public, max-age=60", "--content-type", "application/json" if path.suffix == ".json" else "text/plain" if path.suffix == ".sh" or path.name == "SHA256SUMS" else "application/gzip"]
    if immutable:
        # Resume interrupted releases without ever overwriting a published object.
        existing = json.loads(aws("list-objects-v2", "--bucket", os.environ["R2_BUCKET"], "--prefix", key))
        if any(item["Key"] == key for item in existing.get("Contents", [])):
            verify_upload(path, key)
            return
        args.extend(["--if-none-match", "*"])
    aws(*args)
    verify_upload(path, key)


def verify_upload(path, key):
    # Read back through the authenticated storage API, not a potentially stale CDN.
    with tempfile.TemporaryDirectory(prefix="pontia-upload-check-") as directory:
        downloaded = Path(directory) / "object"
        aws("get-object", "--bucket", os.environ["R2_BUCKET"], "--key", key, str(downloaded))
        if downloaded.read_bytes() != path.read_bytes():
            raise ValueError(f"Uploaded object verification failed: {key}")


def publish_release(directory, version):
    prefix = f"releases/{version}/"
    for path in sorted(directory.iterdir()):
        if path.is_file():
            upload(path, prefix + path.name, immutable=True)
    # A delayed retry of an older release must not roll the stable channel back.
    stable_key = "channels/stable.json"
    existing = json.loads(aws("list-objects-v2", "--bucket", os.environ["R2_BUCKET"], "--prefix", stable_key))
    if any(item["Key"] == stable_key for item in existing.get("Contents", [])):
        with tempfile.TemporaryDirectory(prefix="pontia-stable-check-") as temporary:
            current = Path(temporary) / "stable.json"
            aws("get-object", "--bucket", os.environ["R2_BUCKET"], "--key", stable_key, str(current))
            envelope = json.loads(current.read_bytes())
            manifest = json.loads(base64.b64decode(envelope["manifest"], validate=True))
            if version_order(manifest["version"]) > version_order(version):
                print(f"Stable already points to newer release {manifest['version']}; leaving it unchanged")
                return
            if version_order(manifest["version"]) == version_order(version) and current.read_bytes() != (directory / "manifest.json").read_bytes():
                raise ValueError("Stable already contains a different manifest for this version")
    # The signed envelope is a single object: no manifest/signature update race.
    upload(directory / "manifest.json", stable_key)


def publish_github_release(directory, version):
    # Draft creation and asset upload are separate, recoverable operations.
    try:
        release = json.loads(run("gh", "release", "view", version, "--json", "isDraft,targetCommitish,assets"))
    except subprocess.CalledProcessError:
        run("gh", "release", "create", version, "--draft", "--target", os.environ["GITHUB_SHA"], "--title", version, "--notes-file", "release-notes.md")
        release = {"isDraft": True, "targetCommitish": os.environ["GITHUB_SHA"], "assets": []}
    if release["isDraft"]:
        if release["targetCommitish"] != os.environ["GITHUB_SHA"]:
            raise ValueError("Existing draft targets a different commit")
    else:
        commit = run("gh", "api", f"repos/{os.environ['GITHUB_REPOSITORY']}/commits/{version}", "--jq", ".sha").decode().strip()
        if commit != os.environ["GITHUB_SHA"]:
            raise ValueError("Existing release tag targets a different commit")
    existing = {asset["name"] for asset in release["assets"]}
    for path in sorted(directory.iterdir()):
        if not path.is_file():
            continue
        if path.name not in existing:
            run("gh", "release", "upload", version, str(path))
        with tempfile.TemporaryDirectory(prefix="pontia-github-check-") as temporary:
            run("gh", "release", "download", version, "--pattern", path.name, "--dir", temporary)
            if (Path(temporary) / path.name).read_bytes() != path.read_bytes():
                raise ValueError(f"GitHub asset differs from original build: {path.name}")
    if release["isDraft"]:
        run("gh", "release", "edit", version, "--draft=false")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare-installers", "prepare-release", "publish-installers", "publish-release", "publish-github-release"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--version")
    args = parser.parse_args()
    if args.command.endswith("release") and not args.version:
        parser.error("--version is required")
    if args.command == "prepare-installers":
        prepare_installers(args.directory)
    elif args.command == "prepare-release":
        prepare_release(args.directory, args.version)
    elif args.command == "publish-github-release":
        publish_github_release(args.directory, args.version)
    elif args.command == "publish-installers":
        for name in ("install.sh", "install-edge.sh"):
            upload(args.directory / name, name)
    else:
        publish_release(args.directory, args.version)


if __name__ == "__main__":
    main()
