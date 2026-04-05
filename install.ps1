#Requires -Version 5.1
<#
.SYNOPSIS
    Install or upgrade agent-tools on Windows.
.DESCRIPTION
    Downloads the latest agent-tools release from GitHub and installs it
    to %USERPROFILE%\.agentic\bin. Adds the directory to the user's PATH
    if not already present.
#>

$ErrorActionPreference = "Stop"

$Repo = "nitecon/agent-tools"
$BinaryNames = @("agent-tools.exe", "agent-tools-mcp.exe")
$InstallDir = Join-Path $env:USERPROFILE ".agentic\bin"

# --- Helpers ----------------------------------------------------------------

function Info($msg)  { Write-Host "[INFO]  $msg" -ForegroundColor Green }
function Warn($msg)  { Write-Host "[WARN]  $msg" -ForegroundColor Yellow }
function Fail($msg)  { Write-Host "[ERROR] $msg" -ForegroundColor Red; exit 1 }

# --- Resolve latest version -------------------------------------------------

Info "Resolving latest release..."

try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
    $LatestTag = $release.tag_name
} catch {
    Fail "Could not determine latest release from GitHub: $_"
}

if (-not $LatestTag) {
    Fail "Could not determine latest release tag."
}

Info "Latest version: $LatestTag"

$ArchiveName = "agent-tools-${LatestTag}-x86_64-pc-windows-msvc.zip"
$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$ArchiveName"

# --- Check existing installation --------------------------------------------

$BinaryPath = Join-Path $InstallDir "agent-tools.exe"
if (Test-Path $BinaryPath) {
    try {
        $currentVersion = & $BinaryPath --version 2>$null
        Info "Existing installation found: $currentVersion"
    } catch {
        Info "Existing installation found (version unknown)"
    }
    Info "Upgrading to $LatestTag..."
} else {
    Info "No existing installation found. Installing fresh."
}

# --- Download and extract ---------------------------------------------------

$TmpDir = Join-Path $env:TEMP "agent-tools-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TmpDir -Force | Out-Null

try {
    Info "Downloading $ArchiveName..."
    $archivePath = Join-Path $TmpDir $ArchiveName
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $archivePath -UseBasicParsing

    Info "Extracting..."
    Expand-Archive -Path $archivePath -DestinationPath $TmpDir -Force

    # --- Install ----------------------------------------------------------------

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    foreach ($bin in $BinaryNames) {
        $srcFile = Get-ChildItem -Path $TmpDir -Recurse -Filter $bin | Select-Object -First 1
        if ($srcFile) {
            Copy-Item -Path $srcFile.FullName -Destination (Join-Path $InstallDir $bin) -Force
            Info "Installed $(Join-Path $InstallDir $bin)"
        } else {
            Warn "Binary $bin not found in archive"
        }
    }

} finally {
    Remove-Item -Path $TmpDir -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Add to PATH ------------------------------------------------------------

$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$InstallDir", "User")
    $env:PATH = "$env:PATH;$InstallDir"
    Info "Added $InstallDir to user PATH"
} else {
    Info "$InstallDir already in PATH"
}

# --- Done -------------------------------------------------------------------

Write-Host ""
Info "Installation complete!"
Write-Host ""
Write-Host "  Binaries: $(Join-Path $InstallDir 'agent-tools.exe')"
Write-Host "            $(Join-Path $InstallDir 'agent-tools-mcp.exe')"
Write-Host "  Version:  $LatestTag"
Write-Host ""
Write-Host "Quick start (CLI):"
Write-Host "  agent-tools tree"
Write-Host "  agent-tools symbols src/main.rs"
Write-Host "  agent-tools search MyFunction"
Write-Host ""
Write-Host "Register as MCP server for Claude Code:"
Write-Host "  claude mcp add -s user agent-tools -- `"$(Join-Path $InstallDir 'agent-tools-mcp.exe')`""
Write-Host ""
