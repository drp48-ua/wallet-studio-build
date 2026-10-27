"""Reproduce reviewed presentation crops from private, hash-pinned source PNGs.

Requires Pillow. Originals are never changed; output must be a separate folder.
No quantization, JPEG conversion, sharpening or generated content is used.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image


def prepare(source, output, manifest):
    source, output = Path(source).resolve(), Path(output).resolve()
    if source == output or source in output.parents or output in source.parents:
        raise ValueError("Use separate, non-nested input and output directories")
    rules = json.loads(Path(manifest).read_text(encoding="utf-8"))
    staged = []
    for rule in rules:
        filename = rule["id"] + ".png"
        data = (source / filename).read_bytes()
        if hashlib.sha256(data).hexdigest() != rule["input_sha256"]:
            raise ValueError("Source revision mismatch: " + rule["id"])
        with Image.open(source / filename) as image:
            if image.size != (1536, 969):
                raise ValueError("Unexpected source dimensions")
            crop = image.convert("RGB").crop(tuple(rule["crop_box"]))
            staged.append((filename, crop.resize((1536, 969), Image.Resampling.LANCZOS), rule))
    output.mkdir(parents=True, exist_ok=True)
    for filename, image, rule in staged:
        destination = output / filename
        if destination.exists():
            raise ValueError("Output already exists: " + filename)
        image.save(destination, format="PNG", optimize=True, compress_level=9)
        data = destination.read_bytes()
        if len(data) > 4 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != rule["output_sha256"]:
            raise ValueError("Output verification failed: " + filename)
    return len(staged)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--manifest", default=str(Path(__file__).with_name("library-presentation-crops.json")))
    args = parser.parse_args()
    print("Verified crops:", prepare(args.source, args.output, args.manifest))
