"""Smoke-test the agent as a service against a mock OpenAEV platform.

1. Installs the last released agent as a service with this branch's
   installer script.
2. Hands the running agent an upgrade job carrying this branch's upgrade
   script, as the platform does when an agent reports another version. The
   service must come back up on this build.
3. Hands the upgraded agent an inject job that downloads the last released
   implant and runs it. The implant must run the payload it fetches and
   report its output.

The scripts only trust the production release key, so the mock signs what it
serves with a throwaway key and swaps that key into the scripts it renders.

Usage: python agent-service-smoke.py <build artifact directory, e.g. smoke/linux/x86_64>
"""

import base64
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

REPOSITORY = Path(__file__).resolve().parents[2]
IMPLANT_REPOSITORY = "https://github.com/OpenAEV-Platform/implant.git"
JFROG = "https://filigran.jfrog.io/artifactory"
TENANT = "smoke-tenant"
TOKEN = "smoke-token"
AGENT_ID = "smoke-agent"
INJECT_ID = "smoke-inject"
# The payload prints EXPECTED only if the shell really evaluates it: the text
# of the command alone never contains it.
EXPECTED = "openaev-42"

# Service-mode defaults the platform fills the scripts with.
if sys.platform == "win32":
    OS = "windows"
    INSTALL_DIR = Path(r"C:\Program Files (x86)\Filigran\OAEV Agent")
    SERVICE = "OAEVAgentService"
    BINARY = "openaev-agent.exe"
    PACKAGE = "openaev-agent-installer.exe"
    SCRIPT_EXTENSION = "ps1"
    EXECUTOR = "psh"
    PAYLOAD = 'Write-Output "openaev-$(40+2)"'
else:
    OS = "macos" if sys.platform == "darwin" else "linux"
    INSTALL_DIR = Path("/opt/openaev-agent")
    SERVICE = "openaev-agent"
    BINARY = PACKAGE = "openaev-agent"
    SCRIPT_EXTENSION = "sh"
    EXECUTOR = "sh"
    PAYLOAD = 'echo "openaev-$((40+2))"'


class State:
    """What the mock platform serves and what it has seen."""

    artifact = None  # (content, signature, version) served to the scripts
    implant = None  # implant binary served to the inject job
    job = None  # pending job handed to the agent until it cleans it
    cleaned = {}  # job id -> monotonic time it was cleaned
    registrations = []  # (monotonic time, reported agent version)
    implant_downloads = 0
    callbacks = []  # inject callbacks posted by the implant


class MockPlatform(BaseHTTPRequestHandler):
    def do_POST(self):
        route = self.route()
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        if route == "endpoints/register":
            State.registrations.append((time.monotonic(), body["endpoint_agent_version"]))
            self.reply(200, {"asset_id": "smoke-asset"})
        elif route == "endpoints/jobs":
            self.reply(200, [State.job] if State.job else [])
        elif route == f"injects/execution/{AGENT_ID}/callback/{INJECT_ID}":
            State.callbacks.append(body)
            self.reply(200, {"inject_id": INJECT_ID})
        else:
            self.reply(404, {})

    def do_DELETE(self):
        route = self.route()
        if State.job and route == f"endpoints/jobs/{State.job['asset_agent_id']}":
            State.cleaned[State.job["asset_agent_id"]] = time.monotonic()
            State.job = None
            self.reply(200, {})
        else:
            self.reply(404, {})

    def do_GET(self):
        # The implant command downloads the implant without a token.
        if self.path.startswith(f"/api/tenants/{TENANT}/implant/openaev/{OS}/"):
            State.implant_downloads += 1
            self.send_binary(State.implant)
            return
        route = self.route()
        # agent/executable/openaev/<os>/<arch> or agent/package/openaev/windows/<arch>/service
        if route and route.startswith((f"agent/executable/openaev/{OS}/", f"agent/package/openaev/{OS}/")):
            content, signature, version = State.artifact
            self.send_binary(content, {"X-Signature-Sha256-Rsa": signature, "X-Release-Version": version})
        elif route == f"injects/{INJECT_ID}/{AGENT_ID}/executable-payload":
            command = base64.b64encode(PAYLOAD.encode()).decode()
            self.reply(200, {"payload_type": "Command", "command_executor": EXECUTOR, "command_content": command})
        else:
            self.reply(404, {})

    def route(self):
        """The path below the tenant, or None when the request is not authenticated."""
        prefix = f"/api/tenants/{TENANT}/"
        if self.headers.get("Authorization") != f"Bearer {TOKEN}" or not self.path.startswith(prefix):
            return None
        return self.path[len(prefix):].split("?")[0]

    def send_binary(self, content, headers=None):
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(content)))
        for name, value in (headers or {}).items():
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(content)

    def reply(self, status, body):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class ReleaseKey:
    """Throwaway RSA key standing in for the release signing key."""

    def __init__(self, directory):
        self.path = directory / "release.key"
        openssl("genrsa", "-out", str(self.path), "2048")
        self.pem = openssl("rsa", "-in", str(self.path), "-pubout").decode().strip()
        modulus = openssl("rsa", "-in", str(self.path), "-noout", "-modulus").decode().strip()
        modulus = base64.b64encode(bytes.fromhex(modulus.split("=", 1)[1])).decode()
        # The Windows scripts take the key as RSAKeyValue XML; genrsa uses exponent 65537.
        self.xml = f"<RSAKeyValue><Modulus>{modulus}</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>"

    def sign(self, content):
        return base64.b64encode(openssl("dgst", "-sha256", "-sign", str(self.path), data=content)).decode()


def openssl(*args, data=None):
    return subprocess.run(["openssl", *args], input=data, capture_output=True, check=True).stdout


def render_script(name, url, key):
    """The script as the platform serves it, trusting the throwaway key."""
    text = (REPOSITORY / "installer" / OS / f"{name}.{SCRIPT_EXTENSION}").read_text().replace("\r\n", "\n")
    values = {
        "OPENAEV_URL": url,
        "OPENAEV_TOKEN": TOKEN,
        "OPENAEV_UNSECURED_CERTIFICATE": "false",
        "OPENAEV_WITH_PROXY": "false",
        "OPENAEV_SERVICE_NAME": SERVICE,
        "OPENAEV_INSTALL_DIR": str(INSTALL_DIR),
        "OPENAEV_TENANT_ID": TENANT,
    }
    for placeholder, value in values.items():
        text = text.replace("${" + placeholder + "}", value)
    if SCRIPT_EXTENSION == "ps1":
        keys = f"$TrustedReleaseKeys = @(\n    '{key.xml}'\n)"
        text, swapped = re.subn(r"\$TrustedReleaseKeys = @\(\n.*?\n\)", lambda _: keys, text, flags=re.S)
    else:
        pattern = r"-----BEGIN PUBLIC KEY-----\n.*?-----END PUBLIC KEY-----"
        text, swapped = re.subn(pattern, lambda _: key.pem, text, flags=re.S)
    if swapped != 1:
        sys.exit(f"::error::Expected one release key block in {name}, found {swapped}")
    return text


def implant_command(url, arch):
    """An inject job command: download the implant through the platform API and run it.

    Deliberately not a copy of the command the platform generates, which can
    change without any API change; this only relies on the API routes. The
    agent discards the command's output, so it writes it to LAUNCHER_LOG.
    """
    download = f"{url}/api/tenants/{TENANT}/implant/openaev/{OS}/{arch}?injectId={INJECT_ID}&agentId={AGENT_ID}"
    implant_args = (
        f"--uri {url} --token {TOKEN} --unsecured-certificate false --with-proxy false"
        f" --agent-id {AGENT_ID} --inject-id {INJECT_ID} --tenant-id {TENANT}"
    )
    if OS == "windows":
        return (
            "try { $ErrorActionPreference = 'Stop';"
            f' Invoke-WebRequest -UseBasicParsing -Uri "{download}" -OutFile openaev-implant.exe;'
            f" & .\\openaev-implant.exe {implant_args} }}"
            f" catch {{ $_ | Out-File -Encoding utf8 {LAUNCHER_LOG.name}; exit 1 }}; exit $LASTEXITCODE"
        )
    return (
        f"exec > {LAUNCHER_LOG.name} 2>&1;"
        f' curl -sSf "{download}" -o openaev-implant && chmod +x openaev-implant && ./openaev-implant {implant_args}'
    )


def release_versions(repository):
    """Release tags of a repository, newest first."""
    tags = subprocess.run(
        ["git", "ls-remote", "--tags", "--refs", repository],
        cwd=REPOSITORY, capture_output=True, check=True, text=True,
    ).stdout.split()
    versions = [t.rsplit("/", 1)[1] for t in tags if re.fullmatch(r"refs/tags/\d+\.\d+\.\d+", t)]
    return sorted(versions, key=lambda v: tuple(map(int, v.split("."))), reverse=True)


def download_last_release(repository, product, name, arch):
    """The newest release published in JFrog, as (version, content).

    A tag exists before its CI has passed and its artifacts are promoted, so a
    tag that is not published yet is skipped, as on the CI run of the tag itself.
    """
    stem, dot, extension = name.partition(".")
    for version in release_versions(repository):
        url = f"{JFROG}/{product}/{OS}/{arch}/{stem}-{version}{dot}{extension}"
        try:
            with urllib.request.urlopen(url) as response:
                print(f"Downloaded {url}")
                return version, response.read()
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
            print(f"Not published yet: {url}")
    sys.exit(f"::error::No published {product} release in JFrog")


def run_script(text, directory, name):
    path = directory / f"{name}.{SCRIPT_EXTENSION}"
    path.write_text(text, newline="\n")
    if OS == "windows":
        command = ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(path)]
    else:
        command = ["sudo", "sh", str(path)]
    print(f"--- {name}")
    subprocess.run(command, check=True)


def service_pid():
    if OS == "linux":
        output = subprocess.run(["systemctl", "show", "-p", "MainPID", "--value", SERVICE], capture_output=True, text=True).stdout
        return int(output.strip() or 0)
    if OS == "macos":
        command = ["sudo", "launchctl", "print", f"system/io.filigran.{SERVICE}"]
        pattern = r"\bpid = (\d+)"
    else:
        command = ["sc", "queryex", SERVICE]
        pattern = r"PID\s*:\s*(\d+)"
    match = re.search(pattern, subprocess.run(command, capture_output=True, text=True).stdout)
    return int(match.group(1)) if match else 0


def installed_digest():
    try:
        return hashlib.sha256((INSTALL_DIR / BINARY).read_bytes()).hexdigest()
    except OSError:
        return None


def wait_for(condition, seconds, failure):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if condition():
            return
        time.sleep(2)
    raise TimeoutError(failure)


AGENT_LOG = INSTALL_DIR / "openaev-agent.log"
IMPLANT_LOG = INSTALL_DIR / "runtimes" / "execution-smoke-inject-job" / "openaev-implant.log"
LAUNCHER_LOG = IMPLANT_LOG.with_name("launcher.log")


def read_log(path):
    """A log written by the service, which runs as root or SYSTEM; empty when missing."""
    if OS == "windows":
        return path.read_text(errors="replace") if path.exists() else ""
    result = subprocess.run(["sudo", "cat", str(path)], capture_output=True, text=True, errors="replace")
    return result.stdout if result.returncode == 0 else ""


def check_log(path, startup_line):
    """Fail on a log without its startup line or with any error in it."""
    lines = read_log(path).splitlines()
    if not any(startup_line in line for line in lines):
        raise AssertionError(f"No '{startup_line}' line in {path}")
    errors = [line for line in lines if '"level":"ERROR"' in line]
    if errors:
        raise AssertionError(f"Errors logged in {path}:\n" + "\n".join(errors))


def diagnostics():
    print("--- service")
    if OS == "linux":
        subprocess.run(["systemctl", "status", "--no-pager", SERVICE])
    elif OS == "macos":
        subprocess.run(["sudo", "launchctl", "print", f"system/io.filigran.{SERVICE}"])
    else:
        subprocess.run(["sc", "queryex", SERVICE])
    print(f"--- registrations: {State.registrations}")
    print(f"--- implant downloads: {State.implant_downloads}, callbacks: {json.dumps(State.callbacks)}")
    for log in (AGENT_LOG, INSTALL_DIR / "runner.log", LAUNCHER_LOG, IMPLANT_LOG):
        print(f"--- {log}")
        print(read_log(log) or "(missing)")


def main():
    artifact_dir = Path(sys.argv[1])
    arch = artifact_dir.name
    new_package = (artifact_dir / PACKAGE).read_bytes()
    new_digest = hashlib.sha256((artifact_dir / BINARY).read_bytes()).hexdigest()
    new_version = tomllib.loads((REPOSITORY / "Cargo.toml").read_text())["package"]["version"]
    old_version, old_package = download_last_release("origin", "openaev-agent", PACKAGE, arch)
    implant_name = "openaev-implant.exe" if OS == "windows" else "openaev-implant"
    State.implant = download_last_release(IMPLANT_REPOSITORY, "openaev-implant", implant_name, arch)[1]

    work = Path(tempfile.mkdtemp())
    key = ReleaseKey(work)
    server = ThreadingHTTPServer(("127.0.0.1", 0), MockPlatform)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}"

    try:
        # 1. Install the last release as a service.
        if OS == "linux":
            # The job can start while the runner is still booting, and the
            # installer refuses a systemd that is not up yet.
            subprocess.run(["systemctl", "is-system-running", "--wait"], timeout=300)
        State.artifact = (old_package, key.sign(old_package), old_version)
        run_script(render_script("agent-installer", url, key), work, "agent-installer")
        wait_for(lambda: State.registrations, 120, "The installed agent never registered")
        reported = State.registrations[-1][1]
        if reported != old_version:
            raise AssertionError(f"The installed agent reports {reported}, expected {old_version}")
        old_pid = service_pid()
        print(f"Release {old_version} installed and registered (pid {old_pid})")

        # 2. Hand it an upgrade job, as the platform does on registration.
        State.artifact = (new_package, key.sign(new_package), new_version)
        State.job = {
            "asset_agent_id": "smoke-upgrade",
            "asset_agent_inject": None,
            "asset_agent_agent": AGENT_ID,
            "asset_agent_command": render_script("agent-upgrade", url, key),
        }
        wait_for(lambda: "smoke-upgrade" in State.cleaned, 120, "The agent never picked up the upgrade job")
        upgraded_at = State.cleaned["smoke-upgrade"]
        print("Upgrade job picked up")
        wait_for(
            lambda: installed_digest() == new_digest and service_pid() not in (0, old_pid),
            300,
            "The service was not restarted on this build",
        )
        wait_for(
            lambda: any(at > upgraded_at and v == new_version for at, v in State.registrations),
            120,
            "The upgraded agent never registered",
        )
        recorded = (INSTALL_DIR / "openaev-agent.version").read_text().strip()
        if recorded != new_version:
            raise AssertionError(f"Recorded version is {recorded}, expected {new_version}")
        print(f"Upgraded from {old_version} to this build ({new_version}), service pid {service_pid()}")

        # 3. Hand the upgraded agent an inject job launching the implant.
        State.job = {
            "asset_agent_id": "smoke-inject-job",
            "asset_agent_inject": INJECT_ID,
            "asset_agent_agent": AGENT_ID,
            "asset_agent_command": implant_command(url, arch),
        }
        wait_for(lambda: "smoke-inject-job" in State.cleaned, 120, "The agent never picked up the inject job")
        print("Inject job picked up")
        wait_for(
            lambda: State.callbacks and State.callbacks[-1]["execution_action"] == "complete",
            180,
            "The implant never completed the inject",
        )
        if State.implant_downloads != 1:
            raise AssertionError(f"The implant was downloaded {State.implant_downloads} times, expected once")
        execution = next((c for c in State.callbacks if c["execution_action"] == "command_execution"), None)
        if execution is None or execution["execution_status"] != "SUCCESS":
            raise AssertionError(f"The payload did not run successfully: {execution}")
        output = json.loads(execution["execution_message"])
        if output["exit_code"] != 0 or output["stderr"] or output["stdout"].strip() != EXPECTED:
            raise AssertionError(f"The payload output is {output}, expected stdout {EXPECTED!r}")
        if State.callbacks[-1]["execution_status"] != "INFO":
            raise AssertionError(f"The inject did not complete cleanly: {State.callbacks[-1]}")
        check_log(IMPLANT_LOG, "Starting OpenAEV implant")
        check_log(AGENT_LOG, "Starting OpenAEV agent")
        print(f"The agent launched the implant, which ran the payload and reported {output['stdout'].strip()!r}")
    except (AssertionError, TimeoutError, subprocess.CalledProcessError) as error:
        diagnostics()
        print(f"::error::{error}")
        sys.exit(1)


if __name__ == "__main__":
    main()
