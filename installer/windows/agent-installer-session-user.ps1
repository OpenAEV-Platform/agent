[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
# Can't install the OpenBAS agent in System32 location because NSIS 64 exe
$location = Get-Location
if ($location -like "*C:\Windows\System32*") { cd C:\ }
switch ($env:PROCESSOR_ARCHITECTURE)
{
    "AMD64" {$architecture = "x86_64"; Break}
	"ARM64" {$architecture = "arm64"; Break}
	"x86" {
		switch ($env:PROCESSOR_ARCHITEW6432)
		{
			"AMD64" {$architecture = "x86_64"; Break}
			"ARM64" {$architecture = "arm64"; Break}
		}
	}
}
if ([string]::IsNullOrEmpty($architecture)) { throw "Architecture $env:PROCESSOR_ARCHITECTURE is not supported yet, please create a ticket in openbas github project" }
function Sanitize-UserName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$UserName
    )
    $UserName = $UserName.ToLower()
    $pattern = '[\/\\:\*\?<>\|]'
    return ($UserName -replace $pattern, '')
}
$BasePath = "${OPENBAS_INSTALL_DIR}";
$User = whoami;
$SanitizedUser = Sanitize-UserName -UserName $user;
$isElevated = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevated) {
    $AgentName = "OBASAgent-Session-Administrator-$SanitizedUser"
} else {
    $AgentName = "OBASAgent-Session-$SanitizedUser"
}
$InstallDir = $BasePath + "\" + $AgentName;
$AgentPath = $InstallDir + "\openbas-agent.exe";

try {
    echo "Stop existing agent";
    Get-Process | Where-Object { $_.Path -eq "$AgentPath" } | Stop-Process -Force;

    echo "Downloading and installing OpenBAS Agent...";
    Invoke-WebRequest -Uri "${OPENBAS_URL}/api/agent/package/openbas/windows/${architecture}/session-user" -OutFile "agent-installer-session-user.exe";
    ./agent-installer-session-user.exe /S ~OPENBAS_URL="${OPENBAS_URL}" ~ACCESS_TOKEN="${OPENBAS_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENBAS_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENBAS_WITH_PROXY} ~SERVICE_NAME="${OPENBAS_SERVICE_NAME}" ~INSTALL_DIR="$BasePath";
    Start-Sleep -Seconds 5;
    rm -force ./agent-installer-session-user.exe;
	echo "OpenBAS agent has been successfully installed"
} catch {
    echo "Installation failed"
  	if ((Get-Host).Version.Major -lt 7) { throw "PowerShell 7 or higher is required for installation" }
  	else { echo $_ }
} finally {
    rm -force ./agent-installer-session-user.exe;
  	if ($location -like "*C:\Windows\System32*") { cd C:\Windows\System32 }
}