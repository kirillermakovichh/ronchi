#!/usr/bin/env python3
"""Bounded, local resource observations; Python is not a Ronchi runtime dependency."""
import argparse
import concurrent.futures
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import queue
import shutil
import statistics
import subprocess
import tempfile
import threading
import time
import urllib.parse
import urllib.request


def encoded(value):
    return json.dumps(value, separators=(",", ":")).encode()


def request(url, value=None, headers=None, timeout=10):
    body = None if value is None else encoded(value)
    req = urllib.request.Request(url, body, {"Content-Type": "application/json", **(headers or {})})
    # Environment proxy settings must not send loopback fixture traffic elsewhere.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(req, timeout=timeout) as response:
        raw = response.read(300 * 1024 + 1)
        if len(raw) > 300 * 1024:
            raise RuntimeError("HTTP response exceeded harness bound")
        return json.loads(raw), len(raw), dict(response.headers)


class Source(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        if not 0 < length <= 64 * 1024:
            self.send_error(400)
            return
        call = json.loads(self.rfile.read(length))
        if call["method"] == "getAccountInfo":
            result = {"context": {"slot": 1}, "value": {
                "data": ["A" * (256 * 1024 - 2048), "base64"], "executable": False,
                "lamports": 1, "owner": "11111111111111111111111111111111", "rentEpoch": 0,
            }}
        else:
            result = {"context": {"slot": 1}, "value": 1}
        body = encoded({"jsonrpc": "2.0", "id": call["id"], "result": result})
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def cpu_seconds(text):
    days, _, clock = text.rpartition("-")
    parts = [float(part) for part in clock.split(":")]
    return (int(days) * 86400 if days else 0) + sum(part * 60 ** index for index, part in enumerate(reversed(parts)))


def process_sample(pid):
    """Return birth identity, CPU seconds and resident bytes, or fail explicitly."""
    if platform.system() == "Linux":
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        if fields[0] == "Z":
            raise RuntimeError("Owned process became a zombie")
        return {"birth": fields[19], "cpu_seconds": (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK"),
                "rss_bytes": int(fields[21]) * os.sysconf("SC_PAGE_SIZE")}
    if platform.system() == "Darwin":
        output = subprocess.run(["ps", "-p", str(pid), "-o", "rss=", "-o", "time=", "-o", "lstart="],
                                capture_output=True, text=True, timeout=5, check=True).stdout.split(maxsplit=2)
        if len(output) != 3:
            raise RuntimeError("Owned process sample was unavailable")
        return {"birth": output[2].strip(), "cpu_seconds": cpu_seconds(output[1]), "rss_bytes": int(output[0]) * 1024}
    raise RuntimeError("RSS/CPU sampler supports Linux and macOS only")


class Sampler:
    def __init__(self, identities, interval):
        self.identities = identities
        self.interval = interval
        self.rows = []
        self.errors = []
        self.stop = threading.Event()
        self.thread = None
        self.births = {label: identity["sample_birth"] for label, identity in identities.items()
                       if "sample_birth" in identity}

    def sample(self):
        for label, identity in self.identities.items():
            try:
                sample = process_sample(identity["pid"])
                previous = self.births.setdefault(label, sample["birth"])
                if sample["birth"] != previous:
                    raise RuntimeError("PID identity changed during sampling")
                self.rows.append({"process": label, "monotonic_seconds": time.monotonic(), **sample})
            except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
                self.errors.append({"process": label, "error_type": type(error).__name__})

    def __enter__(self):
        self.sample()
        def run():
            while not self.stop.wait(self.interval):
                self.sample()
        self.thread = threading.Thread(target=run, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.thread.join(6)
        self.sample()

    def report(self):
        result = {"sample_interval_seconds": self.interval, "peak_kind": "sampled_rss_not_os_true_peak",
                  "raw_samples": self.rows, "sample_errors": self.errors, "processes": {}}
        for label in self.identities:
            rows = [row for row in self.rows if row["process"] == label]
            if rows:
                result["processes"][label] = {"sampled_peak_rss_bytes": max(row["rss_bytes"] for row in rows),
                    "cpu_delta_seconds": rows[-1]["cpu_seconds"] - rows[0]["cpu_seconds"],
                    "sample_span_seconds": rows[-1]["monotonic_seconds"] - rows[0]["monotonic_seconds"]}
        return result


class Mcp:
    def __init__(self, lab):
        self.process = subprocess.Popen(lab.argv("mcp", "stdio", selected=False), env=lab.env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.lines = queue.Queue(maxsize=64)
        self.counter = 0
        def read():
            while True:
                line = self.process.stdout.readline(300 * 1024 + 1)
                if not line:
                    self.lines.put(None)
                    return
                if len(line) > 300 * 1024:
                    self.lines.put(RuntimeError("MCP line exceeded harness bound"))
                    return
                self.lines.put(line)
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        try:
            self.call("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "ronchi-resource-harness", "version": "1"}})
            self.process.stdin.write(encoded({"jsonrpc": "2.0", "method": "notifications/initialized"}) + b"\n")
            self.process.stdin.flush()
        except BaseException:
            self.close()
            raise

    def call(self, method, params):
        self.counter += 1
        self.process.stdin.write(encoded({"jsonrpc": "2.0", "id": self.counter, "method": method, "params": params}) + b"\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            line = self.lines.get(timeout=max(0.01, deadline - time.monotonic()))
            if line is None or isinstance(line, Exception):
                raise RuntimeError("MCP adapter ended or exceeded its output bound")
            response = json.loads(line)
            if response.get("id") == self.counter:
                if "error" in response:
                    raise RuntimeError("MCP request returned an error")
                return response["result"], len(line)
        raise TimeoutError("MCP response deadline")

    def close(self):
        if self.process.stdin:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        self.reader.join(timeout=5)


class Lab:
    def __init__(self, args, mode, source):
        self.args, self.mode = args, mode
        self.path = Path(tempfile.mkdtemp(prefix="ronchi-resource-"))
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("RONCHI_")}
        self.id = None
        self.identities = {}
        self.cleaned = False
        self.stopped = False
        try:
            if mode == "surfpool":
                self.cli("tool", "surfpool", "install", "--archive", str(args.surfpool_archive))
            up_args = ["up", "--detach", "--mode", mode, "--rpc-listen", "127.0.0.1:0", "--control-listen", "127.0.0.1:0"]
            if mode == "simulator":
                up_args += ["--read-upstream", source]
            up, _ = self.cli(*up_args, timeout=150)
            self.id = up["lab_id"]
            value = up["result"]
            self.rpc_url = "http://" + value["rpc_address"] + "/"
            self.control_url = "http://" + value["control_address"]
            self.identities = {"core": value["process"]}
            self.identities.update({f"surfpool_child_{i}": identity for i, identity in enumerate(value["owned_children"])})
            for identity in self.identities.values():
                identity["sample_birth"] = process_sample(identity["pid"])["birth"]
            self.startup = {key: value.get(key) for key in ["disk_limits", "versions"]}
            self.startup["execution_mode"] = value["configuration"]["mode"]
            if self.startup["execution_mode"] != mode:
                raise RuntimeError("Disposable Lab returned a different execution mode")
            self.token = (self.path / "ronchi/labs" / self.id / "token").read_text().strip()
        except Exception as error:
            self.startup_failure = {"type": type(error).__name__, "message": "Disposable Lab startup failed"}
            self.cleanup()

    def argv(self, *args, selected=True):
        return [str(self.args.binary), "--project", str(self.path), "--json"] + (
            ["--lab-id", self.id] if selected and self.id else []) + list(args)

    def cli(self, *args, timeout=30):
        command = self.argv(*args)
        output = subprocess.run(command, env=self.env, capture_output=True, timeout=timeout, check=False)
        if output.returncode:
            # Product errors can contain configuration. Keep credentials out of the report.
            raise RuntimeError(f"CLI {args[0]} exited {output.returncode}")
        if len(output.stdout) > 300 * 1024:
            raise RuntimeError("CLI output exceeded harness bound")
        return json.loads(output.stdout), len(output.stdout)

    def control(self, suffix, value):
        return request(f"{self.control_url}/v1/labs/{self.id}/{suffix}", value,
                       {"Authorization": "Bearer " + self.token})

    def rpc(self, method="getSlot", params=None):
        return request(self.rpc_url, {"jsonrpc": "2.0", "id": 1, "method": method, "params": params or []})

    def dashboard_headers(self):
        ticket = self.control("dashboard-ticket", {})[0]["launch_url"]
        ticket = urllib.parse.parse_qs(urllib.parse.urlsplit(ticket).fragment)["ticket"][0]
        _, _, headers = request(f"{self.control_url}/ui/{self.id}/session", {"ticket": ticket}, {"Origin": self.control_url})
        cookie = next(value for key, value in headers.items() if key.lower() == "set-cookie").split(";", 1)[0]
        return {"Cookie": cookie, "Origin": self.control_url}

    def disk(self):
        directory = self.path / "ronchi/labs" / self.id
        files = []
        for path in sorted(directory.rglob("*")):
            if path.is_file() and not path.is_symlink():
                stat = path.stat()
                files.append({"path": str(path.relative_to(directory)), "logical_bytes": stat.st_size,
                    "allocated_bytes": stat.st_blocks * 512})
        return {"files": files, "logical_bytes": sum(item["logical_bytes"] for item in files),
                "allocated_bytes": sum(item["allocated_bytes"] for item in files)}

    def cleanup(self):
        if self.cleaned:
            return True
        try:
            # A bounded CLI shutdown owns signaling and birth/executable verification.
            if not self.stopped:
                self.cli("down", "--force", timeout=30)
                self.stopped = True
            for identity in self.identities.values():
                try:
                    os.kill(identity["pid"], 0)
                except ProcessLookupError:
                    continue
                # A recycled PID is not ours. Conservatively retain the directory.
                return False
            self.cleaned = True
            shutil.rmtree(self.path)
            return True
        except (OSError, RuntimeError, subprocess.SubprocessError):
            return False


def timed_calls(count, concurrency, call):
    def one(_):
        monotonic_start = time.monotonic()
        start = time.perf_counter()
        value, size, *_ = call()
        if isinstance(value, dict) and ("error" in value or value.get("isError")):
            raise RuntimeError("Workload returned a product error")
        return {"latency_ms": (time.perf_counter() - start) * 1000, "output_bytes": size,
                "start_monotonic_seconds": monotonic_start, "end_monotonic_seconds": time.monotonic()}
    start = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
        observations = list(executor.map(one, range(count)))
    elapsed = time.perf_counter() - start
    times = sorted(item["latency_ms"] for item in observations)
    return {"requests": count, "concurrency": concurrency, "elapsed_seconds": elapsed,
            "completed_requests_per_second": count / elapsed, "median_latency_ms": statistics.median(times),
            "p95_latency_ms": times[min(len(times) - 1, int(len(times) * 0.95))], "raw_observations": observations}


def measure(args, mode, source):
    lab = Lab(args, mode, source)
    report = {"mode": mode, "startup": getattr(lab, "startup", {}), "phases": {}, "fixture_path": str(lab.path)}
    try:
        if hasattr(lab, "startup_failure"):
            report["failure"] = lab.startup_failure
            report["active_phase"] = "startup"
            return report
        def phase(name, call, extra_identities=None):
            report["active_phase"] = name
            before = lab.cli("status")[0]["result"]["snapshot_seq"]
            disk_before = lab.disk()
            started = time.perf_counter()
            with Sampler({**lab.identities, **(extra_identities or {})}, args.sample_interval) as sampler:
                result = call()
            elapsed = time.perf_counter() - started
            after = lab.cli("status")[0]["result"]["snapshot_seq"]
            disk_after = lab.disk()
            report["phases"][name] = {"workload": result, "resources": sampler.report(),
                "journal": {"before_seq": before, "after_seq": after, "committed_events": after - before,
                    "observation_elapsed_seconds": elapsed, "committed_events_per_second": (after - before) / elapsed,
                    "logical_byte_delta": disk_after["logical_bytes"] - disk_before["logical_bytes"],
                    "allocated_byte_delta": disk_after["allocated_bytes"] - disk_before["allocated_bytes"]}}
        phase("idle", lambda: {"wait_seconds": args.idle_seconds, "completed": time.sleep(args.idle_seconds) is None})
        phase("rpc_reads", lambda: timed_calls(args.requests, args.concurrency, lab.rpc))
        if mode == "simulator":
            public_key = "11111111111111111111111111111111"
            call = {"jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": [public_key]}
            phase("fake_upstream_direct", lambda: timed_calls(args.requests, args.concurrency, lambda: request(source, call)))
            phase("fake_upstream_via_ronchi", lambda: timed_calls(args.requests, args.concurrency, lambda: lab.rpc("getBalance", [public_key])))
            phase("large_read_response", lambda: timed_calls(max(2, args.requests // 10), args.concurrency,
                lambda: lab.rpc("getAccountInfo", [public_key, {"encoding": "base64"}])))
        # Exercise actual fault selection and recovery over more than one outage cycle.
        lab.control("case/arm", {"case_id": "provider-timeout-burst", "hold_ms": 10,
            "timeout": {"outage_ms": 50, "recovery_ms": 50, "methods": ["getLatestBlockhash"]}})
        def case_load():
            observations = []
            for _ in range(max(4, args.requests // 5)):
                start = time.perf_counter()
                try:
                    value, size, _ = lab.rpc("getLatestBlockhash")
                    observations.append({"elapsed_ms": (time.perf_counter() - start) * 1000,
                                         "output_bytes": size, "rpc_error": "error" in value})
                except OSError as error:
                    observations.append({"elapsed_ms": (time.perf_counter() - start) * 1000,
                                         "transport_error": type(error).__name__})
                time.sleep(0.025)
            return {"method": "getLatestBlockhash", "raw_observations": observations}
        phase("timeout_case", case_load)
        lab.control("case/disarm", {})
        headers = lab.dashboard_headers()
        query = {"contract_version": 1, "request_id": "resource-query", "lab_id": lab.id,
                 "operation": "investigate.summary", "arguments": {}}
        writer_stop = threading.Event()
        writer_result = {"completed": 0, "errors": [], "raw_intervals": []}
        def write_load():
            for _ in range(args.requests * 4):
                if writer_stop.is_set():
                    break
                try:
                    started = time.monotonic()
                    value, _, _ = lab.rpc()
                    if "error" in value:
                        raise RuntimeError("RPC write-load error")
                    writer_result["completed"] += 1
                    writer_result["raw_intervals"].append({"start_monotonic_seconds": started,
                        "end_monotonic_seconds": time.monotonic()})
                except (OSError, RuntimeError) as error:
                    writer_result["errors"].append(type(error).__name__)
                    break
        thread = threading.Thread(target=write_load, daemon=True)
        def dashboard_load():
            thread.start()
            return timed_calls(args.query_count, 1,
                lambda: request(f"{lab.control_url}/ui/{lab.id}/api/investigate", query, headers))
        try:
            phase("dashboard_queries_during_writes", dashboard_load)
        finally:
            writer_stop.set()
            if thread.ident is not None:
                thread.join(15)
        if thread.is_alive():
            raise RuntimeError("Write workload did not stop")
        report["dashboard_concurrent_writes"] = writer_result
        queries = report["phases"]["dashboard_queries_during_writes"]["workload"]["raw_observations"]
        writer_result["queries_overlapping_rpc_calls"] = sum(any(
            write["start_monotonic_seconds"] < query["end_monotonic_seconds"] and
            query["start_monotonic_seconds"] < write["end_monotonic_seconds"]
            for write in writer_result["raw_intervals"]) for query in queries)
        report["disk_while_live"] = lab.disk()
        for operation in ["summary", "timeline"]:
            phase(f"cli_live_{operation}", lambda operation=operation: timed_calls(args.query_count, 1,
                lambda: lab.cli("investigate", operation)))
        mcp = Mcp(lab)
        try:
            for operation in ["summary", "timeline"]:
                phase(f"mcp_live_{operation}", lambda operation=operation: timed_calls(args.query_count, 1,
                    lambda: mcp.call("tools/call", {"name": f"ronchi_investigate_{operation}",
                        "arguments": {"lab_id": lab.id, "arguments": {}}})),
                    {"mcp_adapter": {"pid": mcp.process.pid}})
        finally:
            mcp.close()
        report.pop("active_phase", None)
        lab.cli("down")
        lab.stopped = True
        report["disk_retained"] = lab.disk()
        # These measurements include a new CLI/adapter process; the stopped core is absent.
        for operation in ["summary", "timeline"]:
            report["phases"][f"cli_retained_{operation}"] = {"workload": timed_calls(args.query_count, 1,
                lambda operation=operation: lab.cli("investigate", operation))}
        mcp = Mcp(lab)
        try:
            with Sampler({"mcp_adapter": {"pid": mcp.process.pid}}, args.sample_interval) as sampler:
                workload = timed_calls(args.query_count, 1,
                    lambda: mcp.call("tools/call", {"name": "ronchi_investigate_summary",
                        "arguments": {"lab_id": lab.id, "arguments": {}}}))
            report["phases"]["mcp_retained_summary"] = {"workload": workload, "resources": sampler.report()}
        finally:
            mcp.close()
    except BaseException as error:
        report["failure"] = {"type": type(error).__name__, "message": str(error) if isinstance(error, RuntimeError) else "See bounded phase and command"}
    finally:
        report["cleanup_complete"] = lab.cleanup()
        if report["cleanup_complete"]:
            report.pop("fixture_path", None)
        else:
            report["cleanup_blocker"] = "Owned process termination was not established; fixture retained"
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="Explicit final release executable (debug allowed only with --smoke)")
    parser.add_argument("--output", type=Path, required=True, help="New raw JSON report path")
    parser.add_argument("--build-profile", choices=["release", "debug"], default="release")
    parser.add_argument("--build-revision", default="unknown", help="Commit of the supplied executable; declare explicitly for evidence")
    parser.add_argument("--build-command", default="cargo build --release --locked", help="Actual command used to build the supplied executable")
    parser.add_argument("--smoke", action="store_true", help="Tiny harness check; never resource evidence")
    parser.add_argument("--requests", type=int, default=100)
    parser.add_argument("--query-count", type=int, default=10)
    parser.add_argument("--concurrency", type=int, default=4)
    parser.add_argument("--idle-seconds", type=float, default=3)
    parser.add_argument("--sample-interval", type=float, default=0.1)
    parser.add_argument("--surfpool-archive", type=Path, help="Optional already-downloaded pinned archive; run a separate native mode")
    parser.add_argument("--host-load-note", required=True, help="Describe uncontrolled host work; do not kill user processes")
    args = parser.parse_args()
    if args.build_profile != "release" and not args.smoke:
        parser.error("Resource evidence requires --build-profile release")
    if not (1 <= args.requests <= 10000 and 1 <= args.query_count <= 1000 and 1 <= args.concurrency <= 16
            and 0.1 <= args.idle_seconds <= 300 and 0.02 <= args.sample_interval <= 5):
        parser.error("Workload is outside the bounded harness range")
    args.binary = args.binary.resolve(strict=True)
    if args.output.exists():
        parser.error("Refusing to overwrite an existing report")
    if args.smoke:
        args.requests, args.query_count, args.idle_seconds, args.concurrency = 4, 1, 0.2, 1
    version = subprocess.run([str(args.binary), "--version"], capture_output=True, text=True, timeout=5, check=True).stdout.strip()
    compiler = subprocess.run(["rustc", "--version"], capture_output=True, text=True, timeout=5, check=True).stdout.strip()
    report = {"schema_version": 1, "classification": "harness_smoke_not_resource_evidence" if args.smoke else "resource_observations",
        "harness": {"sha256": hashlib.file_digest(Path(__file__).open("rb"), "sha256").hexdigest()},
        "started_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "host": {"platform": platform.platform(), "machine": platform.machine(), "cpu_count": os.cpu_count(),
                 "python": platform.python_version(), "load_average_at_start": os.getloadavg(), "uncontrolled_load": args.host_load_note},
        "binary": {"path": str(args.binary), "version": version, "build_profile_declared": args.build_profile,
                   "build_revision_declared": args.build_revision, "build_command_declared": args.build_command,
                   "host_rustc_version": compiler, "sha256": hashlib.file_digest(args.binary.open("rb"), "sha256").hexdigest(),
                   "bytes": args.binary.stat().st_size},
        "workload": {key: getattr(args, key) for key in ["requests", "query_count", "concurrency", "idle_seconds", "sample_interval"]},
        "limits": {"rpc_request_bytes": 65536, "rpc_response_bytes": 262144, "rpc_batch_items": 64,
                   "journal_batch_bytes": 1048576, "journal_default_disk_bytes": 1073741824, "shutdown_reserve_bytes": 67108864},
        "method": {"http_connections": "new connection per request; stdlib loopback client", "source": "local fake upstream; no external RPC",
            "core_and_surfpool": "separate PID samples; Python client/source excluded", "mcp": "actual stdio initialize and tools/call",
            "query_payload": "default bounded summary/timeline selection", "warmup": "startup readiness only; no extra warmup",
            "limits": "reported operational defaults; no performance gates", "commands": [
                "BINARY --project FIXTURE --json up --rpc-listen 127.0.0.1:0 --control-listen 127.0.0.1:0",
                "BINARY --project FIXTURE --json --lab-id ID investigate summary|timeline",
                "BINARY --project FIXTURE --json mcp stdio", "BINARY --project FIXTURE --json --lab-id ID down"]}, "runs": []}
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Source)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        source = f"http://127.0.0.1:{server.server_port}/"
        report["runs"].append(measure(args, "simulator", source))
        if args.surfpool_archive:
            report["runs"].append(measure(args, "surfpool", source))
        else:
            report["surfpool_gap"] = "No archive supplied; native mode not measured"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(5)
    report["host"]["load_average_at_end"] = os.getloadavg()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as output:
        json.dump(report, output, indent=2)
        output.write("\n")
    failed = any("failure" in run or not run["cleanup_complete"] for run in report["runs"])
    print(json.dumps({"output": str(args.output), "classification": report["classification"], "complete": not failed}))
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
