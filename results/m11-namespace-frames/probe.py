import argparse, hashlib, json, pathlib, re, subprocess

def leb(data, pos):
    value = 0
    for shift in range(0, 70, 7):
        byte = data[pos]; pos += 1
        value |= (byte & 127) << shift
        if byte < 128:
            return value, pos
    raise ValueError('invalid LEB')

def code_bytes(data):
    assert data[:4] == b'\0asm'
    core = data[4:8] == b'\x01\0\0\0'
    assert core or data[4:8] == b'\x0d\0\x01\0'
    pos, total, bodies = 8, 0, 0
    while pos < len(data):
        section = data[pos]; pos += 1
        size, pos = leb(data, pos)
        end = pos + size
        assert end <= len(data)
        if core and section == 10:
            count, cur = leb(data, pos)
            for _ in range(count):
                length, cur = leb(data, cur)
                total += length; bodies += 1; cur += length
                assert cur <= end
            assert cur == end
        elif not core and section in (1, 4):
            child_total, child_bodies = code_bytes(data[pos:end])
            total += child_total; bodies += child_bodies
        pos = end
    assert pos == len(data)
    return total, bodies

p = argparse.ArgumentParser()
p.add_argument('--suss', type=pathlib.Path, required=True)
p.add_argument('--output', type=pathlib.Path, required=True)
a = p.parse_args()
a.suss = a.suss.resolve(); a.output = a.output.resolve()
a.output.mkdir(parents=True, exist_ok=False)
wit = a.output / 'size.wit'
wit.write_text('package test:size; world w {}')
rows = {}
for grouped in (False, True):
    for n in (4, 24):
        label = f'{"grouped" if grouped else "split"}-{n}'
        definitions = ''.join(f'(def f{i} (fn [a b] a))\n' for i in range(n))
        source = '(ns size (:refer-clojure :exclude []))\n' + ('(do\n' + definitions + ')' if grouped else definitions)
        path = a.output / f'{label}.sus'; path.write_text(source)
        wasm = a.output / f'{label}.wasm'
        command = [str(a.suss), '--no-cache', 'compile', str(path), '-w', str(wit), '-o', str(wasm)]
        run = subprocess.run(command, capture_output=True)
        (a.output / f'{label}.stdout').write_bytes(run.stdout)
        (a.output / f'{label}.stderr').write_bytes(run.stderr)
        row = {'command': command, 'exit_code': run.returncode, 'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        rows[label] = row
        if run.returncode:
            (a.output / 'failed.json').write_text(json.dumps(rows, indent=2) + '\n')
            raise SystemExit(f'{label}: failed ({run.returncode})')
        data = wasm.read_bytes()
        code, bodies = code_bytes(data)
        identities = sorted(set(value.decode() for value in re.findall(rb':compiler-source-sha256 "([0-9a-f]{64})"', data)))
        assert len(identities) == 1, identities
        row["compiler_source_sha256"] = identities[0]
        assert code > 0 and bodies > 0
        row.update(bytes=len(data), code_bytes=code, bodies=bodies, wasm_sha256=hashlib.sha256(data).hexdigest())
split = rows['split-24']['code_bytes'] - rows['split-4']['code_bytes']
grouped = rows['grouped-24']['code_bytes'] - rows['grouped-4']['code_bytes']
report = {'executable': str(a.suss), 'executable_sha256': hashlib.sha256(a.suss.read_bytes()).hexdigest(), 'rows': rows, 'split_code_growth': split, 'grouped_code_growth': grouped, 'extra_code_bytes_per_split_form': (split-grouped)/20}
(a.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
