#!/usr/bin/env python3
"""Retain unmodified bench.py measurements; keep idle waiting a separate command."""
import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

PROGRAMS = ("fib", "word-frequencies", "transduce", "lazy-pipeline",
            "into-vector", "into-map", "string-build")
ISSUE_PROGRAMS = ("lazy-pipeline", "string-build", "into-vector")


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def quiet_since(previous, now, load):
    return (now if previous is None else previous) if load <= 0.75 else None


def wait_idle(output, timeout):
    start, since, samples = time.monotonic(), None, []
    while True:
        now, load = time.monotonic(), os.getloadavg()
        since = quiet_since(since, now, load[0])
        ready = since is not None and now - since >= 60
        samples.append({"time": time.time(), "load": load})
        save(output, {"ready": ready, "threshold": 0.75, "required_seconds": 60,
                      "time": time.time(), "samples": samples})
        print("idle gate", load, "READY" if ready else "waiting", flush=True)
        if ready:
            return
        if now - start >= timeout:
            raise SystemExit("Idle gate timed out; no benchmark was launched")
        time.sleep(10)


def run(source, executable, output, idle):
    gate = json.loads(idle.read_text())
    if not gate.get("ready") or gate.get("threshold") != 0.75 or gate.get("required_seconds") != 60:
        raise SystemExit("A passing standalone idle gate is required")
    if time.time() - gate["time"] > 30 or os.getloadavg()[0] > 0.75:
        raise SystemExit("Idle evidence expired or host load rose; repeat the standalone gate")
    launch_time, launch_load = time.time(), os.getloadavg()
    source, executable, output = source.resolve(), executable.resolve(), output.resolve()
    dirty = subprocess.check_output(["git", "-C", str(source), "status", "--porcelain"], text=True)
    if dirty:
        raise SystemExit("Benchmark source must be clean: " + dirty)
    output.mkdir(parents=True, exist_ok=False)
    (output / "deps.edn").write_text('{:deps {org.clojure/clojure {:mvn/version "1.12.6"}}}\n')
    version = subprocess.run(["mise", "exec", "--", "clojure", "-M", "-e", "(print (clojure-version))"],
                             cwd=output, capture_output=True, text=True, check=True)
    (output / "clojure-version.stdout").write_text(version.stdout)
    (output / "clojure-version.stderr").write_text(version.stderr)
    if version.stdout.strip() != "1.12.6":
        raise SystemExit("Wrong Clojure version: " + version.stdout)
    java = subprocess.run(["mise", "exec", "--", "java", "-version"],
                          cwd=output, capture_output=True, text=True, check=True)
    (output / "java-version.txt").write_text(java.stdout + java.stderr)
    (output / "cpuinfo.txt").write_text(Path("/proc/cpuinfo").read_text())
    script = source / "experiments/numeric-representation/suss-jvm/bench.py"
    sys.argv = [str(script), "5000000", "3"]
    spec = importlib.util.spec_from_file_location("measured_bench", script)
    bench = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bench)
    bench.SUSS = str(executable)
    # Hardware counters are optional in bench.py. Hosted VMs often provide a
    # perf executable without a usable PMU; retain its actual refusal rather
    # than invent instruction counts or change timing/allocation settings.
    perf_probe = None
    if bench.PERF:
        probe = subprocess.run([bench.PERF, "stat", "-x,", "-e", "instructions:u", "--", "true"],
                               capture_output=True, text=True)
        perf_probe = {"returncode": probe.returncode, "stdout": probe.stdout, "stderr": probe.stderr}
        if probe.returncode:
            bench.PERF = None
    receipt = {
        "source_head": subprocess.check_output(["git", "-C", str(source), "rev-parse", "HEAD"], text=True).strip(),
        "source_dirty": dirty, "clojure_version": version.stdout.strip(),
        "java_version": java.stdout + java.stderr,
        "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "bench_sha256": hashlib.sha256(script.read_bytes()).hexdigest(),
        "core_sha256": hashlib.sha256((source / "runtime/core-import/suss/core.sus").read_bytes()).hexdigest(),
        "n": bench.N, "reps": bench.REPS, "cases": bench.CASES,
        "reductions": bench.REDUCTIONS, "programs": bench.PROGRAMS, "loops": bench.LOOPS,
        "idle_receipt": gate, "start_time": launch_time, "start_load": launch_load,
        "machine": platform.uname()._asdict(), "cpus": os.cpu_count(),
        "java_tool_options": os.environ.get("JAVA_TOOL_OPTIONS", ""),
        "perf_probe": perf_probe, "instructions_available": bool(bench.PERF), "completed": False,
    }
    save(output / "receipt.json", receipt)
    load = bench.Load()
    load.start()
    try:
        jvm, jvm_allocated = bench.jvm_times(str(output))
        save(output / "jvm.json", {"times": jvm, "allocated": jvm_allocated})
        suss, instructions, allocated = bench.suss_times(str(output))
        save(output / "suss.json", {"times": suss, "instructions": instructions, "allocated": allocated})
        load.report()
        bench.table(suss, instructions, allocated, jvm, jvm_allocated)
        receipt["completed"] = True
    finally:
        load.done.set()
        receipt.update(end_time=time.time(), end_load=os.getloadavg(), load_samples=load.samples)
        save(output / "receipt.json", receipt)


def evaluate(baseline, candidate, required_programs=None):
    required = list(required_programs if required_programs is not None else ["lazy-pipeline", "string-build"])
    if not required or any(name not in ISSUE_PROGRAMS for name in required):
        raise ValueError("Only the existing lazy-pipeline/string-build/into-vector issue gates may be selected")
    failures, ratios, regressions = [], {}, {}
    unresolved = []
    for label, data in [("baseline", baseline), ("candidate", candidate)]:
        receipt = data["receipt"]
        if not receipt.get("completed") or receipt.get("source_dirty"):
            failures.append(label + " run is incomplete or dirty")
        if receipt.get("clojure_version") != "1.12.6" or receipt.get("n") != 5000000 or receipt.get("reps") != 3:
            failures.append(label + " benchmark configuration differs")
        if [row[0] for row in receipt.get("programs", [])] != list(PROGRAMS):
            failures.append(label + " program coverage differs")
        idle = receipt.get("idle_receipt", {})
        if not idle.get("ready") or idle.get("threshold") != 0.75 or idle.get("required_seconds") != 60:
            failures.append(label + " standalone idle evidence is missing")
        samples = receipt.get("load_samples", [])
        if not samples or max(samples) > 3 or receipt.get("start_load", [math.inf])[0] > 0.75:
            failures.append(label + " host was not quiet")
    if baseline["receipt"]["bench_sha256"] != candidate["receipt"]["bench_sha256"]:
        failures.append("Benchmark source differs between revisions")
    for name in PROGRAMS:
        suss, jvm = candidate["suss"]["times"][name], candidate["jvm"]["times"][name]
        if not all(math.isfinite(x) for x in [*suss, *jvm]) or not (suss[0] > suss[1] >= 0 and jvm[0] > jvm[1] >= 0):
            unresolved.append(name)
            if name in required:
                failures.append(name + " timing is below resolution or invalid")
            continue
        ratios[name] = suss[0] / jvm[0]
        if name in required and ratios[name] > 5:
            failures.append(name + " exceeds 5x JVM")
    size = candidate["suss"]["allocated"].get("string-build")
    if size is None or not math.isfinite(size) or not 0 <= size < 200000:
        failures.append("String allocation is not established below 200000 bytes/run")
    for name, _ in candidate["receipt"]["programs"]:
        before, after = baseline["suss"]["times"][name], candidate["suss"]["times"][name]
        if not all(math.isfinite(x) for x in [*before, *after]) or not (before[0] > before[1] >= 0 and after[0] > after[1] >= 0):
            failures.append(name + " comparison is below resolution or invalid")
            continue
        # Use the benchmark's measured spreads, not a new percentage tolerance.
        regressions[name] = {"delta_seconds": after[0] - before[0], "combined_spread": before[1] + after[1]}
        if after[0] - before[0] > before[1] + after[1]:
            failures.append(name + " regresses beyond both measured spreads")
    return {"timing_and_allocation_pass": not failures, "failures": failures,
            "required_programs": required, "ratios": ratios, "unresolved_ratios": unresolved,
            "unmet_program_targets": [name for name, ratio in ratios.items() if ratio > 5],
            "string_bytes_per_run": size, "program_comparisons": regressions}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    wait = commands.add_parser("wait")
    wait.add_argument("--output", type=Path, required=True)
    wait.add_argument("--timeout", type=float, default=1200)
    measure = commands.add_parser("run")
    for name in ["source", "executable", "output", "idle"]:
        measure.add_argument("--" + name, type=Path, required=True)
    compare = commands.add_parser("compare")
    for name in ["baseline", "candidate", "output"]:
        compare.add_argument("--" + name, type=Path, required=True)
    compare.add_argument("--require-program", action="append", choices=ISSUE_PROGRAMS)
    args = parser.parse_args()
    if args.command == "wait":
        wait_idle(args.output, args.timeout)
    elif args.command == "run":
        run(args.source, args.executable, args.output, args.idle)
    else:
        runs = [{name: json.loads((directory / (name + ".json")).read_text())
                 for name in ["receipt", "suss", "jvm"]} for directory in [args.baseline, args.candidate]]
        result = evaluate(*runs, args.require_program)
        save(args.output, result)
        print(json.dumps(result, indent=2))
        if result["failures"]:
            raise SystemExit(1)


if __name__ == "__main__":
    main()
