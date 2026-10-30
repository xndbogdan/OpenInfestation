"""Build Remastered.pak, the sounds the port adds,
from the sound files the builder is given.

    python -I scripts/build-remastered-pak.py [--manifest PATH] [--out PATH]
        [--root NAME=DIR ...] [--work DIR] [--release]

The manifest (ambience/manifest.json) names each sound's source
under a named root (your own sound folders: INF_SOUNDS_PACKS4 and
INF_SOUNDS_SPLICE, or --root here), its licence
and its processing. Each is converted with ffmpeg to Ogg Vorbis at 44.1 kHz,
mono or stereo, trimmed, filtered and brought to its loudness, then packed
with credits.txt into the pak the port reads (platform_audio/remastered_pak.rs).
The sounds and the pak never enter git; the sources are only read.

A sound marked "shippable": false (no licence lets the port ship it)
goes only into a pak built for your own game. --release builds the
pak a release's zip carries without them, and without their credits; the
port leaves the layers that would play them silent.
"""
import argparse
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

MAGIC = b"INFPAK1\0"
VERSION = 1
REPO = Path(__file__).resolve().parents[1]


def convert(source, target, sound):
    """One sound through ffmpeg: trim, filters, loudness, 44.1 kHz Vorbis."""
    args = ["ffmpeg", "-v", "error", "-y"]
    if "start" in sound:
        args += ["-ss", str(sound["start"])]
    if "seconds" in sound:
        args += ["-t", str(sound["seconds"])]
    args += ["-i", str(source)]
    chain = [sound["filters"]] if sound.get("filters") else []
    # Integrated loudness (EBU R128), the true peak kept under -2 dBFS.
    chain.append(f"loudnorm=I={sound.get('loudness', -24)}:TP=-2:LRA=11")
    layout = "mono" if sound.get("channels", 2) == 1 else "stereo"
    chain.append(f"aresample=44100,aformat=channel_layouts={layout}")
    # Under 3 s loudnorm only scales, so a short crack can pass its peak:
    # a limiter at -2 dBFS on the sound as it will be.
    chain.append("alimiter=limit=0.794:level=false:latency=true")
    # The audio alone, untagged: some sources embed a cover picture, which
    # ffmpeg would add as a Theora stream the port's decoder (lewton) cannot
    # skip.
    args += ["-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "-1",
             "-af", ",".join(chain), "-ac", str(sound.get("channels", 2)), "-ar", "44100",
             "-c:a", "libvorbis", "-q:a", str(sound.get("quality", 5)), str(target)]
    subprocess.run(args, check=True)


def pack(out, manifest_name, files):
    """The pak: header, index, then the files as they are."""
    header = len(MAGIC) + 4 + 4 + len(manifest_name.encode()) + 4
    index_size = sum(2 + len(name.encode()) + 8 + 8 + 32 for name, _ in files)
    offset = header + index_size
    index = b""
    for name, data in files:
        encoded = name.encode()
        index += struct.pack("<H", len(encoded)) + encoded
        index += struct.pack("<QQ", offset, len(data)) + hashlib.sha256(data).digest()
        offset += len(data)
    with open(out, "wb") as pak:
        pak.write(MAGIC + struct.pack("<I", VERSION))
        pak.write(struct.pack("<I", len(manifest_name.encode())) + manifest_name.encode())
        pak.write(struct.pack("<I", len(files)) + index)
        for _, data in files:
            pak.write(data)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--manifest", type=Path, default=REPO / "ambience/manifest.json")
    parser.add_argument("--out", type=Path, default=REPO / "retail/Remastered.pak")
    parser.add_argument("--root", action="append", default=[], metavar="NAME=DIR")
    parser.add_argument("--work", type=Path, help="keep the converted sounds here")
    parser.add_argument("--release", action="store_true",
                        help="leave out the sounds no licence lets us ship")
    args = parser.parse_args()
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg is needed to convert the sounds")
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    # Manifest roots name environment variables (${INF_SOUNDS_...}).
    roots = {name: os.path.expandvars(folder) for name, folder in manifest.get("roots", {}).items()}
    for override in args.root:
        name, _, folder = override.partition("=")
        roots[name] = folder
    # Each sound takes the manifest's defaults under its own fields.
    sounds = [{**manifest.get("defaults", {}), **sound} for sound in manifest.get("sounds", [])]
    if args.release:
        left = [sound["name"] for sound in sounds if not sound.get("shippable", True)]
        sounds = [sound for sound in sounds if sound.get("shippable", True)]
        if left:
            print(f"--release leaves out {len(left)} sounds no licence lets us ship")
    names = [sound["name"] for sound in sounds]
    if len(set(names)) != len(names) or "credits.txt" in names:
        sys.exit("each sound needs a name of its own")
    missing = []
    for sound in sounds:
        root, _, rest = sound["source"].partition(":")
        if root not in roots:
            sys.exit(f"{sound['name']}: no root named {root!r}")
        if "$" in roots[root]:
            sys.exit(f"root {root!r} is {roots[root]}: set it or pass --root {root}=DIR")
        sound["path"] = Path(roots[root]) / rest
        if not sound["path"].is_file():
            missing.append(str(sound["path"]))
    if missing:
        sys.exit("missing sources:\n  " + "\n  ".join(missing))
    work = args.work or Path(tempfile.mkdtemp(prefix="remastered-"))
    files = []
    for sound in sounds:
        target = work / sound["name"]
        target.parent.mkdir(parents=True, exist_ok=True)
        convert(sound["path"], target, sound)
        files.append((sound["name"], target.read_bytes()))
        print(f"{sound['name']}: {target.stat().st_size // 1024} KB")
    lines = []
    for credit in manifest.get("credits", []):
        if isinstance(credit, str):
            lines.append(credit)
        elif credit.get("shippable", True) or not args.release:
            lines.append(credit["text"])
    credits = "\n".join(lines) + "\n"
    files.append(("credits.txt", credits.encode("utf-8")))
    pack(args.out, manifest["name"], files)
    print(f"{args.out}: {len(files)} files, {args.out.stat().st_size // 1024} KB, from {manifest['name']}")
    if not args.work:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
