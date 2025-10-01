[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
$isElevatedPowershell = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevatedPowershell -like "False") { throw "PowerShell 'Run as Administrator' is required for installation" }
# Can't install the OpenAEV agent in System32 location because NSIS 64 exe
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
if ([string]::IsNullOrEmpty($architecture)) { throw "Architecture $env:PROCESSOR_ARCHITECTURE is not supported yet, please create a ticket in openaev github project" }

echo "Downloading and installing OpenAEV Agent..."
try {
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/agent/package/openaev/windows/${architecture}/service" -OutFile "openaev-installer.exe";
    ./openaev-installer.exe /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="${OPENAEV_INSTALL_DIR}" | Out-Null;

	echo "OpenAEV agent has been successfully installed"
} catch {
    echo "Installation failed"
  	if ((Get-Host).Version.Major -lt 7) { throw "PowerShell 7 or higher is required for installation" }
  	else { echo $_ }
} finally {
    rm -force ./openaev-installer.exe;
  	if ($location -like "*C:\Windows\System32*") { cd C:\Windows\System32 }
}
