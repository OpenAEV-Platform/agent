"""Smoke-test the agent as a service-user installation against a mock OpenAEV platform.

1. Installs the last released agent with this branch's service-user installer
   script, as a service running under a throwaway account.
2. Hands the running agent an upgrade job carrying this branch's upgrade
   script, as the platform does when an agent reports another version. The
   script runs as the service account and must bring the service back up on
   this build.
3. Hands the upgraded agent an inject job that downloads the last released
   implant and runs it. The implant must run the payload it fetches and
   report its output.
4. Uninstalls the agent as the documentation tells an administrator to. The
   service and the agent directory must be gone.

Whatever the outcome, the account and what it owns are removed at the end, so
that a runner which is not thrown away after the job stays clean.

The scripts only trust the production release key, so the mock signs what it
serves with a throwaway key and swaps that key into the scripts it renders.

Linux and Windows only: the macOS user scripts cannot be installed unattended.

Usage: python agent-service-user-smoke.py <build artifact directory, e.g. smoke/linux/x86_64>
"""

import base64
import hashlib
import json
import os
import re
import secrets
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
# Account the service runs as.
ACCOUNT = "oaevsmoke"

if sys.platform == "win32":
    OS = "windows"
    BINARY = "openaev-agent.exe"
    PACKAGE = "openaev-agent-installer-service-user.exe"
    SCRIPT_EXTENSION = "ps1"
    EXECUTOR = "psh"
    PAYLOAD = 'Write-Output "openaev-$(40+2)"'
    # The values the documentation gives for this installation.
    SERVICE_NAME = "OAEVAgent-Service"
    USER = f"{os.environ['COMPUTERNAME']}\\{ACCOUNT}"
    PASSWORD = "Aa1-" + secrets.token_hex(12)
    # The installer joins this to the profile of the account, which the upgrade
    # script is given in full. It names the agent directory after the service
    # and the sanitized account, and so does the Windows service.
    INSTALL_DIR = ".openaev"
    UPGRADE_DIR = Path(os.environ["SystemDrive"] + "\\") / "Users" / ACCOUNT / INSTALL_DIR
    SANITIZED_USER = re.sub(r"[/\\:*?<>|]", "", USER.lower())
    AGENT_DIR = UPGRADE_DIR / f"{SERVICE_NAME}-{SANITIZED_USER}"
    SERVICE = AGENT_DIR.name
else:
    if sys.platform == "darwin":
        sys.exit("::error::The service-user installation is not smoke-tested on macOS")
    OS = "linux"
    BINARY = PACKAGE = "openaev-agent"
    SCRIPT_EXTENSION = "sh"
    EXECUTOR = "sh"
    PAYLOAD = 'echo "openaev-$((40+2))"'
    PASSWORD = ""
    # The values the documentation gives for this installation.
    SERVICE_NAME = "openaev-agent"
    HOME = Path("/home") / ACCOUNT
    # The installer adds the account to this directory, the upgrade script takes it as is.
    INSTALL_DIR = ".local/openaev-agent-service"
    AGENT_DIR = HOME / f".local/openaev-agent-service-{ACCOUNT}"
    UPGRADE_DIR = AGENT_DIR
    SERVICE = f"{ACCOUNT}-openaev-agent"

AGENT_LOG = AGENT_DIR / "openaev-agent.log"
IMPLANT_LOG = AGENT_DIR / "runtimes" / "execution-smoke-inject-job" / "openaev-implant.log"
LAUNCHER_LOG = IMPLANT_LOG.with_name("launcher.log")

# Grants the account the right to log on as a service, which `sc create` does
# not, then logs it on once so that its profile exists: the agent directory
# lives in it, and only a profile created by a logon belongs to the account.
WINDOWS_ACCOUNT_SCRIPT = r"""
param([string]$Name, [string]$Password)
$ErrorActionPreference = 'Stop'
$sid = (New-Object System.Security.Principal.NTAccount($Name)).Translate([System.Security.Principal.SecurityIdentifier]).Value
$template = Join-Path $env:TEMP 'oaev-rights.inf'
$database = Join-Path $env:TEMP 'oaev-rights.sdb'
secedit /export /cfg $template /areas USER_RIGHTS | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'secedit export failed' }
(Get-Content $template) -replace '^(SeServiceLogonRight\s*=.*)$', "`$1,*$sid" | Set-Content $template -Encoding Unicode
secedit /configure /db $database /cfg $template /areas USER_RIGHTS | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'secedit configure failed' }
$credential = New-Object System.Management.Automation.PSCredential(".\$Name", (ConvertTo-SecureString $Password -AsPlainText -Force))
Start-Process -FilePath cmd.exe -ArgumentList '/c', 'exit' -Credential $credential -LoadUserProfile -Wait
"""


# Takes the "Log on as a service" right back, then removes the account and its
# profile. The profile can only be removed once nothing runs as the account.
WINDOWS_CLEANUP_SCRIPT = r"""
param([string]$Name)
$sid = $null
try { $sid = (New-Object System.Security.Principal.NTAccount($Name)).Translate([System.Security.Principal.SecurityIdentifier]).Value } catch { }
if ($sid) {
    $template = Join-Path $env:TEMP 'oaev-rights-cleanup.inf'
    $database = Join-Path $env:TEMP 'oaev-rights-cleanup.sdb'
    secedit /export /cfg $template /areas USER_RIGHTS | Out-Null
    (Get-Content $template) -replace ",\*$sid", '' -replace "\*$sid,", '' | Set-Content $template -Encoding Unicode
    secedit /configure /db $database /cfg $template /areas USER_RIGHTS | Out-Null
    Get-CimInstance Win32_UserProfile | Where-Object { $_.SID -eq $sid } | Remove-CimInstance
}
net user $Name /delete
"""


class State:
    """What the mock platform serves and what it has seen."""

    key = None  # ReleaseKey signing what is served
    artifacts = {}  # "package" or "executable" -> (content, signature, version) served to the scripts
    implant = None  # implant binary served to the inject job
    job = None  # pending job handed to the agent until it cleans it
    cleaned = {}  # job id -> monotonic time it was cleaned
    registrations = []  # (monotonic time, reported agent version)
    implant_downloads = 0
    callbacks = []  # inject callbacks posted by the implant


def serve(version, package, executable=None):
    """Serve a release: the installer package, and the bare executable the upgrade script fetches on Windows."""
    State.artifacts = {
        kind: (content, State.key.sign(content), version)
        for kind, content in (("package", package), ("executable", executable))
        if content
    }


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
        # agent/executable/openaev/<os>/<arch> or agent/package/openaev/windows/<arch>/service-user
        if route and route.startswith((f"agent/executable/openaev/{OS}/", f"agent/package/openaev/{OS}/")):
            artifact = State.artifacts.get(route.split("/")[1])
            if artifact is None:
                self.reply(404, {})
                return
            content, signature, version = artifact
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
        # The scripts are readable by the service account, the key must not be.
        self.path.chmod(0o600)
        self.pem = openssl("rsa", "-in", str(self.path), "-pubout").decode().strip()
        modulus = openssl("rsa", "-in", str(self.path), "-noout", "-modulus").decode().strip()
        modulus = base64.b64encode(bytes.fromhex(modulus.split("=", 1)[1])).decode()
        # The Windows scripts take the key as RSAKeyValue XML; genrsa uses exponent 65537.
        self.xml = f"<RSAKeyValue><Modulus>{modulus}</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>"

    def sign(self, content):
        return base64.b64encode(openssl("dgst", "-sha256", "-sign", str(self.path), data=content)).decode()


def openssl(*args, data=None):
    return subprocess.run(["openssl", *args], input=data, capture_output=True, check=True).stdout


def render_script(name, url, key, install_dir):
    """The script as the platform serves it, trusting the throwaway key."""
    text = (REPOSITORY / "installer" / OS / f"{name}.{SCRIPT_EXTENSION}").read_text().replace("\r\n", "\n")
    values = {
        "OPENAEV_URL": url,
        "OPENAEV_TOKEN": TOKEN,
        "OPENAEV_UNSECURED_CERTIFICATE": "false",
        "OPENAEV_WITH_PROXY": "false",
        "OPENAEV_SERVICE_NAME": SERVICE_NAME,
        "OPENAEV_INSTALL_DIR": str(install_dir),
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


def create_account(directory):
    """The account the service runs as."""
    if OS == "windows":
        script = directory / "account.ps1"
        script.write_text(WINDOWS_ACCOUNT_SCRIPT)
        subprocess.run(["net", "user", ACCOUNT, PASSWORD, "/add"], check=True)
        subprocess.run(
            [
                "powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                "-File", str(script), "-Name", ACCOUNT, "-Password", PASSWORD,
            ],
            check=True,
        )
    else:
        subprocess.run(
            ["sudo", "useradd", "--create-home", "--home-dir", str(HOME), "--user-group", "--shell", "/bin/sh", ACCOUNT],
            check=True,
        )


def run_installer(text, directory):
    path = directory / f"agent-installer-service-user.{SCRIPT_EXTENSION}"
    path.write_text(text, newline="\n")
    path.chmod(0o644)
    if OS == "windows":
        command = ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(path)]
        command += ["-User", USER, "-Password", PASSWORD]
    else:
        command = ["sudo", "sh", str(path), "--user", ACCOUNT, "--group", ACCOUNT]
    print("--- agent-installer-service-user")
    subprocess.run(command, check=True)


def service_pid():
    if OS == "linux":
        output = subprocess.run(["systemctl", "show", "-p", "MainPID", "--value", SERVICE], capture_output=True, text=True).stdout
        return int(output.strip() or 0)
    match = re.search(r"PID\s*:\s*(\d+)", subprocess.run(["sc", "queryex", SERVICE], capture_output=True, text=True).stdout)
    return int(match.group(1)) if match else 0


def service_registered():
    """Whether the service manager still knows the service."""
    if OS == "linux":
        output = subprocess.run(["systemctl", "list-unit-files", f"{SERVICE}.service"], capture_output=True, text=True).stdout
        return f"{SERVICE}.service" in output
    return subprocess.run(["sc", "query", SERVICE], capture_output=True).returncode != 1060  # ERROR_SERVICE_DOES_NOT_EXIST


def exists(path):
    """Whether a path exists, in a directory the runner may not be allowed into."""
    if OS == "windows":
        return path.exists()
    return subprocess.run(["sudo", "test", "-e", str(path)]).returncode == 0


def uninstall():
    """Uninstall as the documentation tells an administrator to.

    Windows ships an uninstaller; on Linux the documentation lists the systemd
    commands and the directory to remove.
    """
    print("--- uninstall")
    if OS == "windows":
        # The uninstaller copies itself aside to delete its own directory and
        # returns at once: the directory disappearing is the end of the run.
        subprocess.run([str(AGENT_DIR / "uninstall.exe"), "/S"], check=True)
    else:
        for action in (["stop", SERVICE], ["disable", SERVICE], ["daemon-reload"], ["reset-failed"]):
            subprocess.run(["sudo", "systemctl", *action], check=True)
        subprocess.run(["sudo", "rm", "-rf", str(AGENT_DIR)], check=True)


def cleanup(directory):
    """Remove the account and what it owns, whatever became of the run. Best effort."""
    print("--- cleanup")
    if OS == "windows":
        subprocess.run(["sc", "stop", SERVICE])
        subprocess.run(["sc", "delete", SERVICE])
        script = directory / "cleanup.ps1"
        script.write_text(WINDOWS_CLEANUP_SCRIPT)
        subprocess.run(
            ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(script), "-Name", ACCOUNT]
        )
    else:
        subprocess.run(["sudo", "systemctl", "stop", SERVICE])
        subprocess.run(["sudo", "systemctl", "disable", SERVICE])
        subprocess.run(["sudo", "rm", "-f", f"/etc/systemd/system/{SERVICE}.service"])
        subprocess.run(["sudo", "systemctl", "daemon-reload"])
        subprocess.run(["sudo", "pkill", "-KILL", "-u", ACCOUNT])
        subprocess.run(["sudo", "userdel", "--remove", "--force", ACCOUNT])


def read_file(path):
    """A file written by the service, which runs as another account; None when missing."""
    if OS == "windows":
        try:
            return path.read_bytes()
        except OSError:
            return None
    result = subprocess.run(["sudo", "cat", str(path)], capture_output=True)
    return result.stdout if result.returncode == 0 else None


def installed_digest():
    content = read_file(AGENT_DIR / BINARY)
    return hashlib.sha256(content).hexdigest() if content else None


def wait_for(condition, seconds, failure):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if condition():
            return
        time.sleep(2)
    raise TimeoutError(failure)


def read_log(path):
    """A log written by the service; empty when missing."""
    return (read_file(path) or b"").decode(errors="replace")


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
    else:
        subprocess.run(["sc", "queryex", SERVICE])
    print(f"--- registrations: {State.registrations}")
    print(f"--- implant downloads: {State.implant_downloads}, callbacks: {json.dumps(State.callbacks)}")
    for log in (AGENT_LOG, AGENT_DIR / "runner.log", LAUNCHER_LOG, IMPLANT_LOG):
        print(f"--- {log}")
        print(read_log(log) or "(missing)")


def main():
    artifact_dir = Path(sys.argv[1])
    arch = artifact_dir.name
    new_package = (artifact_dir / PACKAGE).read_bytes()
    new_binary = (artifact_dir / BINARY).read_bytes()
    new_digest = hashlib.sha256(new_binary).hexdigest()
    new_version = tomllib.loads((REPOSITORY / "Cargo.toml").read_text())["package"]["version"]
    old_version, old_package = download_last_release("origin", "openaev-agent", PACKAGE, arch)
    implant_name = "openaev-implant.exe" if OS == "windows" else "openaev-implant"
    State.implant = download_last_release(IMPLANT_REPOSITORY, "openaev-implant", implant_name, arch)[1]

    work = Path(tempfile.mkdtemp())
    # The service account must reach the scripts, but not the signing key.
    work.chmod(0o711)
    scripts = work / "scripts"
    scripts.mkdir()
    scripts.chmod(0o755)
    State.key = key = ReleaseKey(work)
    server = ThreadingHTTPServer(("127.0.0.1", 0), MockPlatform)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}"

    try:
        # 1. Install the last release as a service running as the account.
        if OS == "linux":
            # The job can start while the runner is still booting, and the
            # installer refuses a systemd that is not up yet.
            subprocess.run(["systemctl", "is-system-running", "--wait"], timeout=300)
        create_account(work)
        # On Linux the executable is the package. On Windows the installer is
        # the package, and the upgrade script fetches the bare executable.
        serve(old_version, old_package, old_package if PACKAGE == BINARY else None)
        run_installer(render_script("agent-installer-service-user", url, key, INSTALL_DIR), scripts)
        wait_for(lambda: State.registrations, 120, "The installed agent never registered")
        reported = State.registrations[-1][1]
        if reported != old_version:
            raise AssertionError(f"The installed agent reports {reported}, expected {old_version}")
        old_pid = service_pid()
        print(f"Release {old_version} installed and registered (pid {old_pid})")

        # 2. Hand it an upgrade job, as the platform does on registration. It
        # runs as the service account, which is not allowed to restart the
        # service as root would: the scripts have their own way to do it.
        serve(new_version, new_package, new_binary)
        State.job = {
            "asset_agent_id": "smoke-upgrade",
            "asset_agent_inject": None,
            "asset_agent_agent": AGENT_ID,
            "asset_agent_command": render_script("agent-upgrade-service-user", url, key, UPGRADE_DIR),
        }
        wait_for(lambda: "smoke-upgrade" in State.cleaned, 120, "The agent never picked up the upgrade job")
        upgraded_at = State.cleaned["smoke-upgrade"]
        print("Upgrade job picked up")
        # On Linux the service only comes back once systemd restarts it, 60 seconds after the kill.
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
        recorded = (read_file(AGENT_DIR / "openaev-agent.version") or b"").decode().strip()
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

        # 4. Uninstall: nothing of the agent may remain.
        State.job = None
        uninstall()
        wait_for(lambda: not exists(AGENT_DIR), 60, f"{AGENT_DIR} was not removed")
        wait_for(lambda: not service_registered(), 60, f"The service {SERVICE} is still registered")
        print("Agent uninstalled: service unregistered and agent directory removed")
    except (AssertionError, TimeoutError, subprocess.CalledProcessError) as error:
        diagnostics()
        # The command of a failed step carries the password of the account.
        message = str(error).replace(PASSWORD, "***") if PASSWORD else str(error)
        print(f"::error::{message}")
        sys.exit(1)
    finally:
        cleanup(work)


if __name__ == "__main__":
    main()
