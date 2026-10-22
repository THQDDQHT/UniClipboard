#!/usr/bin/env python3
"""Package the existing Windows release binaries with the portable marker."""

import argparse
import json
from pathlib import Path
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target",
        required=True,
        choices=["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"],
    )
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    release = root / "target" / args.target / "release"
    version = json.loads((root / "package.json").read_text(encoding="utf-8"))["version"]
    arch = "x64" if args.target.startswith("x86_64-") else "arm64"
    gui = next(
        (release / name for name in ["UniClipboard.exe", "uniclipboard.exe"]
         if (release / name).is_file()),
        None,
    )
    daemon = release / "uniclipd.exe"
    if gui is None or not daemon.is_file():
        raise FileNotFoundError(f"Both GUI and daemon binaries are required in {release}")
    readme = root / "packaging/windows/portable/README.txt"
    output = release / "bundle/portable" / f"UniClipboard_{version}_{arch}-portable.zip"
    output.parent.mkdir(parents=True, exist_ok=True)
    # Write only distribution files, never a user's existing data directory.
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(gui, "UniClipboard.exe")
        archive.write(daemon, "uniclipd.exe")
        archive.writestr("portable.dat", b"")
        archive.write(readme, "README.txt")
    print(f"Built portable zip: {output}")


if __name__ == "__main__":
    main()
