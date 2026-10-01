#!/usr/bin/env python3
"""Build the closed editing catalog from Inspect.java output; translations stay in FTL."""
import argparse
import csv
import json
from pathlib import Path


def main():
    """Emit deterministic data into an explicitly selected research/output directory."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("model", type=Path)
    parser.add_argument("values", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    model = json.loads(args.model.read_text())
    rows = set()
    for owner in model:
        for field in owner["fields"]:
            if field["changeable"] and not field["derived"]:
                rows.add((owner["class"], field["namespace"], field["xml"], field["type"],
                          str(int(field["property"])), field["since"]))
    with args.values.open() as source:
        values = {(row[0], row[1]) for row in list(csv.reader(source, delimiter="\t"))[1:]}
    args.output.mkdir(parents=True, exist_ok=True)
    for name, header, records in [("fields.tsv", ("class", "namespace", "field", "type", "property", "profile"), rows),
                                   ("enums.tsv", ("type", "value"), values)]:
        with (args.output / name).open("w", newline="") as target:
            writer = csv.writer(target, delimiter="\t", lineterminator="\n")
            writer.writerow(header)
            writer.writerows(sorted(records))


if __name__ == "__main__":
    main()
