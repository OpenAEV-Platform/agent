[Net.ServicePointManager]::SecurityProtocol += [Net.SecurityProtocolType]::Tls12;
$isElevatedPowershell = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($isElevatedPowershell -like "False") { throw "PowerShell 'Run as Administrator' is required for installation" }
# Can't install the OpenAEV agent in System32 location because NSIS 64 exe
$location = Get-Location
if ($location -like "*C:\Windows\System32*") { Set-Location C:\ }
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

Write-Output "Downloading and installing OpenAEV Agent..."
try {
    Invoke-WebRequest -Uri "${OPENAEV_URL}/api/tenants/${OPENAEV_TENANT_ID}/agent/package/openaev/windows/${architecture}/service" -OutFile "openaev-installer.exe";
    ./openaev-installer.exe /S ~OPENAEV_URL="${OPENAEV_URL}" ~ACCESS_TOKEN="${OPENAEV_TOKEN}" ~UNSECURED_CERTIFICATE=${OPENAEV_UNSECURED_CERTIFICATE} ~WITH_PROXY=${OPENAEV_WITH_PROXY} ~SERVICE_NAME="${OPENAEV_SERVICE_NAME}" ~INSTALL_DIR="${OPENAEV_INSTALL_DIR}" ~TENANT_ID="${OPENAEV_TENANT_ID}"  | Out-Null;
	Write-Output "OpenAEV agent has been successfully installed"
} catch {
    Write-Output "Installation failed"
    Write-Output "Note: PowerShell 7 or higher is recommended. If the issue persists, consider upgrading."
    Write-Output $_
} finally {
    Start-Sleep -Seconds 2
    Remove-Item -Force ./openaev-installer.exe;
  	if ($location -like "*C:\Windows\System32*") { Set-Location C:\Windows\System32 }
}
