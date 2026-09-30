#!/bin/sh
# Development-only pinned ClojureScript/Node reader boundary oracle.
set -eu
reader_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
reader_pin=c4295f303100bbf5afac449242d30bca1126f1a1
[ "$(git -C "$reader_root/clojurescript" rev-parse HEAD)" = "$reader_pin" ] || { echo 'wrong oracle source pin' >&2; exit 1; }
[ -z "$(git -C "$reader_root/clojurescript" status --porcelain)" ] || { echo 'dirty oracle source' >&2; exit 1; }
cd "$reader_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/reader.js" :output-dir "out/reader-cljs" :optimizations :none :source-map false}' -c suss-oracle.reader-boundary
node out/reader.js > out/reader-observations.json
python3 - <<'PY'
import json
import sys
sys.path.insert(0, '../../scripts')
from oracle_transport import PIN, unique, value

with open('reader-cases.json') as f:
    corpus = json.load(f, object_pairs_hook=unique)
with open('out/reader-observations.json') as f:
    observed = json.load(f, object_pairs_hook=unique)
if set(corpus) != {'schema', 'upstream', 'cases'} or type(corpus['schema']) is not int or corpus['schema'] != 1 or corpus['upstream'] != PIN:
    raise ValueError('invalid reader corpus schema or pin')
if not isinstance(corpus['cases'], list) or not corpus['cases']:
    raise ValueError('reader corpus must contain cases')
ids = set()
for entry in corpus['cases']:
    if set(entry) != {'id', 'source', 'expected'} or not isinstance(entry['id'], str) or not entry['id'] or entry['id'] in ids or not isinstance(entry['source'], str) or not entry['source']:
        raise ValueError('invalid or duplicate reader case')
    ids.add(entry['id'])
    value(entry['expected'])
expected = {'schema': corpus['schema'], 'upstream': corpus['upstream'], 'cases': [
    {'id': entry['id'], 'value': entry['expected']} for entry in corpus['cases']]}
if observed != expected:
    raise ValueError('pinned reader observations differ from the reviewed corpus')
print(str(len(corpus['cases'])) + ' pinned reader observations match exactly')
PY
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$reader_root/Cargo.toml" -p suss-compile --test reader_runtime --locked -- --test-threads=2
