# commonplace launcher for Windows PowerShell: picks the prebuilt binary and runs it.
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$skill = Resolve-Path (Join-Path $here "..")
$env:COMMONPLACE_SKILL_DIR = "$skill"
$arch = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq "Arm64") { "aarch64" } else { "x86_64" }
$candidates = @("$skill\bin\commonplace-$arch-pc-windows-msvc.exe", "$skill\..\..\bin\targets\commonplace-$arch-pc-windows-msvc.exe")
if ($env:COMMONPLACE_BIN) { $candidates = @($env:COMMONPLACE_BIN) + $candidates }
if ($env:BRAIN_BIN) { $candidates = @($env:BRAIN_BIN) + $candidates }
foreach ($bin in $candidates) {
  if (Test-Path $bin) { & $bin @args; exit $LASTEXITCODE }
}
$onPath = Get-Command commonplace -ErrorAction SilentlyContinue
if ($onPath) { & commonplace @args; exit $LASTEXITCODE }
Write-Error "commonplace: no binary for $arch-pc-windows-msvc in $skill\bin. See references/cli.md (manual mode)."
exit 127
