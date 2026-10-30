"""Smoke-test the agent as a session-user installation against a mock OpenAEV platform.

1. Installs the last released agent with this branch's session-user installer
   script, as the agent of a user session.
2. Hands the running agent an upgrade job carrying this branch's upgrade
   script, as the platform does when an agent reports another version. The
   script runs inside the agent's own session and must bring the agent back
   up on this build.
3. Hands the upgraded agent an inject job that downloads the last released
   implant and runs it. The implant must run the payload it fetches and
   report its output.
4. Uninstalls the agent as the documentation tells its user to. The agent
   directory and what starts the agent must be gone.

Whatever the outcome, the account and what it owns are removed at the end, so
that a runner which is not thrown away after the job stays clean.

The scripts only trust the production release key, so the mock signs what it
serves with a throwaway key and swaps that key into the scripts it renders.

On Linux the agent is a systemd user service of a throwaway account. On
Windows it runs as the runner's own account, which is elevated, and the agent
is then started by a scheduled task at logon. With non-elevated, it runs as a
throwaway standard account instead and gets a registry Run key, started by the
installer.

Linux and Windows only: the macOS user scripts cannot be installed unattended.

Usage: python agent-session-user-smoke.py <build artifact directory, e.g. smoke/linux/x86_64> [non-elevated]
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
# Account the agent runs as, on Linux and on Windows when not elevated.
ACCOUNT = "oaevsmoke"
NON_ELEVATED = sys.argv[2:] == ["non-elevated"]

# Runs a script as the account, from a logon that also creates its profile, and
# shows its output. The installer of a standard user has to be run this way:
# the runner is an administrator.
WINDOWS_RUN_AS_SCRIPT = r"""
param([string]$Name, [string]$Password, [string]$Script)
$ErrorActionPreference = 'Stop'
$credential = New-Object System.Management.Automation.PSCredential(".\$Name", (ConvertTo-SecureString $Password -AsPlainText -Force))
$output = "$Script.out"
$errors = "$Script.err"
$process = Start-Process -FilePath powershell.exe -ArgumentList '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', "`"$Script`"" -Credential $credential -LoadUserProfile -WorkingDirectory (Split-Path $Script) -RedirectStandardOutput $output -RedirectStandardError $errors -Wait -PassThru
Get-Content $output, $errors -ErrorAction SilentlyContinue
exit $process.ExitCode
"""

if sys.platform == "win32":
    OS = "windows"
    BINARY = "openaev-agent.exe"
    PACKAGE = "openaev-agent-installer-session-user.exe"
    SCRIPT_EXTENSION = "ps1"
    EXECUTOR = "psh"
    PAYLOAD = 'Write-Output "openaev-$(40+2)"'
    # The values the documentation gives for this installation: a folder of
    # the profile of the user.
    SERVICE_NAME = "OAEVAgent-Session"
    SYSTEM_DRIVE = Path(os.environ["SystemDrive"] + "\\")
    if NON_ELEVATED:
        USER = f"{os.environ['COMPUTERNAME']}\\{ACCOUNT}"
        PASSWORD = "Aa1-" + secrets.token_hex(12)
        BASE_DIR = SYSTEM_DRIVE / "Users" / ACCOUNT / ".openaev"
        # The account must reach its scripts, which the runner's temporary directory does not allow.
        SCRIPTS_DIR = SYSTEM_DRIVE / "oaev-smoke"
        ELEVATION = ""
    else:
        USER = subprocess.run(["whoami"], capture_output=True, check=True, text=True).stdout.strip()
        PASSWORD = ""
        BASE_DIR = Path.home() / ".openaev"
        ELEVATION = "Administrator-"
    SANITIZED_USER = re.sub(r"[/\\:*?<>|]", "", USER.lower())
    # The scripts name the agent directory and the scheduled task after the
    # service and the sanitized account, and flag an elevated account.
    AGENT_DIR = BASE_DIR / f"{SERVICE_NAME}-{ELEVATION}{SANITIZED_USER}"
    TASK = AGENT_DIR.name
    # They also keep the installed version next to the agent directories, not in them.
    VERSION_DIR = BASE_DIR
    INSTALL_DIR = UPGRADE_DIR = str(BASE_DIR)
else:
    if sys.platform == "darwin":
        sys.exit("::error::The session-user installation is not smoke-tested on macOS")
    if NON_ELEVATED:
        sys.exit("::error::Non-elevated only applies to Windows: the Linux agent already runs as a standard account")
    OS = "linux"
    BINARY = PACKAGE = "openaev-agent"
    SCRIPT_EXTENSION = "sh"
    EXECUTOR = "sh"
    PAYLOAD = 'echo "openaev-$((40+2))"'
    PASSWORD = ""
    # The values the documentation gives for this installation.
    SERVICE_NAME = "openaev-agent-session"
    HOME = Path("/home") / ACCOUNT
    # Relative to the home of the account, for the install and the upgrade script alike.
    INSTALL_DIR = UPGRADE_DIR = ".local/openaev-agent-session"
    AGENT_DIR = HOME / INSTALL_DIR
    VERSION_DIR = AGENT_DIR
    UNIT = SERVICE_NAME

AGENT_LOG = AGENT_DIR / "openaev-agent.log"
IMPLANT_LOG = AGENT_DIR / "runtimes" / "execution-smoke-inject-job" / "openaev-implant.log"
LAUNCHER_LOG = IMPLANT_LOG.with_name("launcher.log")


# Removes what a non-elevated run leaves: the processes of the account, then the
# account and its profile, which cannot go while anything runs as the account.
WINDOWS_CLEANUP_SCRIPT = r"""
param([string]$Name)
Get-Process -IncludeUserName -ErrorAction SilentlyContinue | Where-Object { $_.UserName -like "*\$Name" } | Stop-Process -Force
$sid = $null
try { $sid = (New-Object System.Security.Principal.NTAccount($Name)).Translate([System.Security.Principal.SecurityIdentifier]).Value } catch { }
if ($sid) {
    Get-CimInstance Win32_UserProfile | Where-Object { $_.SID -eq $sid } | Remove-CimInstance
}
net user $Name /delete
"""


class State:
    """What the mock platform serves and what it has seen."""

    key = None  # ReleaseKey signing what is served
    artifact = None  # (content, signature, version) served to the scripts
    implant = None  # implant binary served to the inject job
    job = None  # pending job handed to the agent until it cleans it
    cleaned = {}  # job id -> monotonic time it was cleaned
    registrations = []  # (monotonic time, reported agent version)
    implant_downloads = 0
    callbacks = []  # inject callbacks posted by the implant


def serve(version, content):
    State.artifact = (content, State.key.sign(content), version)


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
        # agent/executable/openaev/<os>/<arch> or agent/package/openaev/windows/<arch>/session-user
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
        # The scripts are readable by the agent's account, the key must not be.
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
        "OPENAEV_INSTALL_DIR": install_dir,
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


def user_environment():
    runtime = f"/run/user/{uid()}"
    return [f"HOME={HOME}", f"XDG_RUNTIME_DIR={runtime}", f"DBUS_SESSION_BUS_ADDRESS=unix:path={runtime}/bus"]


def uid():
    return subprocess.run(["id", "-u", ACCOUNT], capture_output=True, check=True, text=True).stdout.strip()


def user_systemctl(*args):
    """`systemctl --user` of the account running the agent."""
    command = ["sudo", "-u", ACCOUNT, "env", *user_environment(), "systemctl", "--user", *args]
    return subprocess.run(command, capture_output=True, text=True)


def create_account(scripts):
    """The account the agent runs as. On Linux it has a user manager of its own, as when it is logged in."""
    if OS == "windows":
        subprocess.run(["net", "user", ACCOUNT, PASSWORD, "/add"], check=True)
        scripts.mkdir()
        subprocess.run(["icacls", str(scripts), "/grant", f"{ACCOUNT}:(OI)(CI)M"], check=True)
        return
    subprocess.run(
        ["sudo", "useradd", "--create-home", "--home-dir", str(HOME), "--user-group", "--shell", "/bin/sh", ACCOUNT],
        check=True,
    )
    subprocess.run(["sudo", "loginctl", "enable-linger", ACCOUNT], check=True)
    wait_for(
        lambda: user_systemctl("is-system-running").stdout.strip() in ("running", "degraded"),
        60,
        "The user manager never came up",
    )


def run_script(text, directory, name):
    """Run a script as the account the agent runs as."""
    path = directory / f"{name}.{SCRIPT_EXTENSION}"
    path.write_text(text, newline="\n")
    path.chmod(0o644)
    if NON_ELEVATED:
        helper = directory / "run-as.ps1"
        helper.write_text(WINDOWS_RUN_AS_SCRIPT)
        command = [
            "powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(helper),
            "-Name", ACCOUNT, "-Password", PASSWORD, "-Script", str(path),
        ]
    elif OS == "windows":
        command = ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(path)]
    else:
        command = ["sudo", "-u", ACCOUNT, "env", *user_environment(), "sh", str(path)]
    print(f"--- {name}")
    subprocess.run(command, check=True)


def exists(path):
    """Whether a path exists, in a directory the runner may not be allowed into."""
    if OS == "windows":
        return path.exists()
    return subprocess.run(["sudo", "test", "-e", str(path)]).returncode == 0


def agent_registered():
    """Whether what starts the agent at logon is still there."""
    if OS == "linux":
        return f"{UNIT}.service" in user_systemctl("list-unit-files", f"{UNIT}.service").stdout
    if NON_ELEVATED:
        # The Run key is in the registry hive of the account, which is not loaded while it is logged off.
        return False
    return subprocess.run(["schtasks", "/Query", "/TN", TASK], capture_output=True).returncode == 0


def uninstall(scripts):
    """Uninstall as the documentation tells the user to.

    Windows ships an uninstaller, to run once the agent is stopped. On Linux
    the documentation lists the systemd commands and the directory to remove.
    """
    print("--- uninstall")
    if OS == "linux":
        for action in (["stop", UNIT], ["disable", UNIT], ["daemon-reload"], ["reset-failed"]):
            result = user_systemctl(*action)
            if result.returncode != 0:
                raise AssertionError(f"systemctl --user {' '.join(action)} failed: {result.stderr.strip()}")
        subprocess.run(["sudo", "-u", ACCOUNT, "rm", "-rf", str(AGENT_DIR)], check=True)
        return
    # The uninstaller copies itself aside to delete its own directory and
    # returns at once: the directory disappearing is the end of the run. It
    # runs as the user of the agent, whose scheduled task or Run key it removes.
    text = (
        "Get-Process openaev-agent -ErrorAction SilentlyContinue"
        f" | Where-Object {{ $_.Path -eq '{AGENT_DIR / BINARY}' }} | Stop-Process -Force\n"
        f"$process = Start-Process -FilePath '{AGENT_DIR / 'uninstall.exe'}' -ArgumentList '/S' -Wait -PassThru\n"
        "exit $process.ExitCode\n"
    )
    run_script(text, scripts, "agent-uninstall")


def cleanup(directory):
    """Remove the account and what it owns, whatever became of the run. Best effort."""
    print("--- cleanup")
    if OS == "windows":
        if NON_ELEVATED:
            script = directory / "cleanup.ps1"
            script.write_text(WINDOWS_CLEANUP_SCRIPT)
            subprocess.run(
                ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(script), "-Name", ACCOUNT]
            )
            subprocess.run(["cmd", "/c", "rmdir", "/s", "/q", str(SCRIPTS_DIR)])
        else:
            # The runner's own account stays: undo what the agent left in it.
            subprocess.run(["schtasks", "/End", "/TN", TASK])
            subprocess.run(["schtasks", "/Delete", "/TN", TASK, "/F"])
            subprocess.run(
                [
                    "powershell", "-NoProfile", "-NonInteractive", "-Command",
                    f"Get-Process openaev-agent -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -eq '{AGENT_DIR / BINARY}' }} | Stop-Process -Force;"
                    f" Remove-Item -Recurse -Force -ErrorAction SilentlyContinue '{AGENT_DIR}', '{VERSION_DIR / 'openaev-agent.version'}'",
                ]
            )
        return
    user_systemctl("stop", UNIT)
    user_systemctl("disable", UNIT)
    subprocess.run(["sudo", "loginctl", "disable-linger", ACCOUNT])
    subprocess.run(["sudo", "loginctl", "terminate-user", ACCOUNT])
    subprocess.run(["sudo", "pkill", "-KILL", "-u", ACCOUNT])
    subprocess.run(["sudo", "userdel", "--remove", "--force", ACCOUNT])


def agent_pid():
    if OS == "linux":
        return int(user_systemctl("show", "-p", "MainPID", "--value", UNIT).stdout.strip() or 0)
    # No service on Windows: the agent is a process started at logon.
    query = (
        "(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq '"
        + str(AGENT_DIR / BINARY)
        + "' } | Select-Object -First 1).ProcessId"
    )
    output = subprocess.run(
        ["powershell", "-NoProfile", "-NonInteractive", "-Command", query], capture_output=True, text=True
    ).stdout
    match = re.search(r"\d+", output)
    return int(match.group()) if match else 0


def read_file(path):
    """A file written by the agent, which may run as another account; None when missing."""
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
    """A log written by the agent; empty when missing."""
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
    print("--- agent")
    if OS == "linux":
        status = user_systemctl("status", "--no-pager", UNIT)
        print(status.stdout, status.stderr)
    elif not NON_ELEVATED:
        # A standard account has a registry Run key instead of a task.
        subprocess.run(["schtasks", "/Query", "/TN", TASK, "/V", "/FO", "LIST"])
    print(f"--- pid: {agent_pid()}")
    print(f"--- registrations: {State.registrations}")
    print(f"--- implant downloads: {State.implant_downloads}, callbacks: {json.dumps(State.callbacks)}")
    for log in (AGENT_LOG, AGENT_DIR / "runner.log", LAUNCHER_LOG, IMPLANT_LOG):
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
    # The account must reach the scripts, but not the signing key.
    work.chmod(0o711)
    if NON_ELEVATED:
        scripts = SCRIPTS_DIR
    else:
        scripts = work / "scripts"
        scripts.mkdir()
        scripts.chmod(0o755)
    State.key = key = ReleaseKey(work)
    server = ThreadingHTTPServer(("127.0.0.1", 0), MockPlatform)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{server.server_address[1]}"

    try:
        # 1. Install the last release as a session agent.
        if OS == "linux":
            # The job can start while the runner is still booting, and the
            # installer refuses a systemd that is not up yet.
            subprocess.run(["systemctl", "is-system-running", "--wait"], timeout=300)
        if OS == "linux" or NON_ELEVATED:
            create_account(scripts)
        serve(old_version, old_package)
        run_script(render_script("agent-installer-session-user", url, key, INSTALL_DIR), scripts, "agent-installer-session-user")
        wait_for(lambda: State.registrations, 120, "The installed agent never registered")
        reported = State.registrations[-1][1]
        if reported != old_version:
            raise AssertionError(f"The installed agent reports {reported}, expected {old_version}")
        old_pid = agent_pid()
        print(f"Release {old_version} installed and registered (pid {old_pid})")

        # 2. Hand it an upgrade job, as the platform does on registration. The
        # script runs inside the agent's session and restarts the agent itself.
        serve(new_version, new_package)
        State.job = {
            "asset_agent_id": "smoke-upgrade",
            "asset_agent_inject": None,
            "asset_agent_agent": AGENT_ID,
            "asset_agent_command": render_script("agent-upgrade-session-user", url, key, UPGRADE_DIR),
        }
        wait_for(lambda: "smoke-upgrade" in State.cleaned, 120, "The agent never picked up the upgrade job")
        upgraded_at = State.cleaned["smoke-upgrade"]
        print("Upgrade job picked up")
        wait_for(
            lambda: installed_digest() == new_digest and agent_pid() not in (0, old_pid),
            300,
            "The agent was not restarted on this build",
        )
        wait_for(
            lambda: any(at > upgraded_at and v == new_version for at, v in State.registrations),
            120,
            "The upgraded agent never registered",
        )
        recorded = (read_file(VERSION_DIR / "openaev-agent.version") or b"").decode().strip()
        if recorded != new_version:
            raise AssertionError(f"Recorded version is {recorded}, expected {new_version}")
        print(f"Upgraded from {old_version} to this build ({new_version}), pid {agent_pid()}")

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
        uninstall(scripts)
        wait_for(lambda: not exists(AGENT_DIR), 60, f"{AGENT_DIR} was not removed")
        wait_for(lambda: not agent_registered(), 60, "What starts the agent is still registered")
        wait_for(lambda: agent_pid() == 0, 60, "The agent is still running")
        print("Agent uninstalled: agent directory and logon start removed, agent stopped")
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
