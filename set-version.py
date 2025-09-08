#!/usr/bin/python3
import tomllib
from pathlib import Path
import re

def replace(file: str, pattern: str, replacement: str):
    file=Path(file)
    text = file.read_text()
    new_text = re.sub(pattern, replacement, text)
    if text != new_text:
        file.write_text(new_text)
        print(f"Updated {file}")
    else:
        print(f"No changes needed in {file}")

with open("variables.toml", "rb") as f:
    file= tomllib.load(f)
version = file["version"]["version"]

replace("Dockerfile.deploy", r"LABEL version=\"\d+\.\d+\.\d+\"", f"LABEL version=\"{version}\"")
replace("Dockerfile", r"LABEL version=\"\d+\.\d+\.\d+\"", f"LABEL version=\"{version}\"")
replace("Cargo.toml", r"version\s*=\s*\"\d+\.\d+\.\d+\"", f"version = \"{version}\"")

oapid = file["openapi"]

replace("oapigen.ps1", r"$OPENAPI_GENERATOR_VERSION=\"\d+\.\d+\.\d+\"", f"$OPENAPI_GENERATOR_VERSION=\"{oapid["oapigen-version"]}\"")
replace("oapigen.ps1", r"$SPEC_PATH=\"\d+\.\d+\.\d+\"", f"$SPEC_PATH=\"{oapid["oapi-spec"]}\"")

replace("oapigen.sh", r"OPENAPI_GENERATOR_VERSION=\"\d+\.\d+\.\d+\"", f"OPENAPI_GENERATOR_VERSION=\"{oapid["oapigen-version"]}\"")
replace("oapigen.sh", r"SPEC_PATH=\"\d+\.\d+\.\d+\"", f"SPEC_PATH=\"{oapid["oapi-spec"]}\"")