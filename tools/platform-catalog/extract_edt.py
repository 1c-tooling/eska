#!/usr/bin/env python3
"""Inspect installed EDT without modifying its installation or launching a workspace."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import subprocess
from urllib.parse import unquote, urlparse
import zipfile


def inspect(args):
    """Collect active bundles, serializer fields and paired palette captions in playground."""
    repository = Path(__file__).resolve().parents[2]
    playground = (repository.parent / 'eska-playground').resolve()
    output = Path(args.output).resolve()
    if not output.is_relative_to(playground) or output == playground:
        raise ValueError('Output must be a new subdirectory of ../eska-playground')
    output.mkdir(parents=True, exist_ok=False)
    edt = Path(args.edt).resolve()
    bundles = {}
    with (edt / 'configuration/org.eclipse.equinox.simpleconfigurator/bundles.info').open() as source:
        for row in csv.reader(line for line in source if not line.startswith('#')):
            name, version, uri, *_ = row
            parsed = urlparse(uri.removeprefix('reference:'))
            path = Path(unquote(parsed.path))
            if not path.is_absolute():
                path = edt / path
            bundles[name] = path.resolve()
    for profile in ['8.3.27', '8.5.1']:
        if 'com._1c.g5.v8.dt.platform_v' + profile not in bundles:
            raise ValueError('Missing active EDT profile ' + profile)
    command = (["distrobox", "enter", args.container, "--"] if args.container else [])
    command += [args.java, '--class-path', ':'.join(map(str, bundles.values())),
                str(Path(__file__).with_name('Inspect.java'))]
    with (output / 'model.json').open('w') as stdout:
        subprocess.run(command, stdout=stdout, check=True)
    sources = []
    for name, path in sorted(bundles.items()):
        if name not in ['com._1c.g5.v8.dt.metadata', 'com._1c.g5.v8.dt.mcore',
                        'com._1c.g5.v8.dt.md.export.xml', 'com._1c.g5.v8.dt.md.ui',
                        'com._1c.g5.v8.dt.md.ui.extension', 'com._1c.g5.v8.dt.mcore.ui',
                        'com._1c.g5.v8.dt.platform_v8.3.27', 'com._1c.g5.v8.dt.platform_v8.5.1']:
            continue
        sources.append({'bundle': name, 'file': path.name, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
        with zipfile.ZipFile(path) as archive:
            for entry in archive.namelist():
                if entry.startswith('localization/') and 'FeatureNames' in entry and entry.endswith('.properties'):
                    # Known relative file names only; never extract archive paths directly.
                    target = output / name / Path(entry).name
                    target.parent.mkdir(exist_ok=True)
                    target.write_bytes(archive.read(entry))
    (output / 'sources.json').write_text(json.dumps(sources, indent=2) + '\n')
    print(output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--edt', required=True, help='EDT installation containing configuration/bundles.info')
    parser.add_argument('--java', default='java', help='Java executable, inside the container when selected')
    parser.add_argument('--container', help='Optional Distrobox container with access to these paths')
    parser.add_argument('--output', required=True, help='New output directory inside the sibling playground')
    inspect(parser.parse_args())
