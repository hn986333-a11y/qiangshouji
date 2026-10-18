$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
foreach ($tool in @('cargo', 'rustc', 'node', 'pnpm')) {
  if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Missing $tool. Read SOCKS5-CHANGES.md prerequisites." }
}
function Run-Step([scriptblock] $Command) {
  & $Command
  if ($LASTEXITCODE -ne 0) { throw "Build step failed with code $LASTEXITCODE" }
}
Run-Step { pnpm install --frozen-lockfile }
Run-Step { pnpm test }
Run-Step { cargo fmt --all --check }
Run-Step { cargo test --workspace --locked }
Run-Step { pnpm tauri build --bundles nsis }
Write-Host 'Installer: target\release\bundle\nsis\*-setup.exe'
Write-Host 'Application: target\release\apple-store-inventory-monitor.exe'
