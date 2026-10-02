# Package/protocol launch contract used by the official Codex Windows TUI:
# https://github.com/openai/codex/blob/1182834d1325c14878349ff8ca85d4d696a70ef7/codex-rs/tui/src/app/history_ui.rs
$ErrorActionPreference = 'Stop'
$installed = Get-AppxPackage -Name OpenAI.Codex -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $installed) { exit 1 }

$metadata = Get-AppxPackageManifest -Package $installed.PackageFullName
$entry = $metadata.Package.Applications.Application | Where-Object {
    @($_.Extensions.Extension) | Where-Object {
        $_.Category -eq 'windows.protocol' -and $_.Protocol.Name -eq 'codex'
    }
} | Select-Object -First 1
if ($null -eq $entry -or [string]::IsNullOrWhiteSpace($entry.Executable)) { exit 1 }

$launcher = Join-Path $installed.InstallLocation $entry.Executable
$directory = Split-Path -Parent $launcher
if (-not (Test-Path -LiteralPath $launcher -PathType Leaf)) { exit 1 }
if (-not (Test-Path -LiteralPath (Join-Path $directory 'resources\app.asar') -PathType Leaf)) { exit 1 }

# Use the declared package launcher with its app bundle, not an internal shim
# or a stale registry executable path. The desktop app handles activation.
Start-Process -FilePath $launcher -WorkingDirectory $directory -ArgumentList @('resources\app.asar', 'codex://')
