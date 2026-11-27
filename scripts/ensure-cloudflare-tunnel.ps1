# Ensure cloudflared is installed, configured, and running for ax Command Center.
#
# Usage:
#   .\scripts\ensure-cloudflare-tunnel.ps1 -Hostname ax.example.com
#   .\scripts\ensure-cloudflare-tunnel.ps1 -Hostname ax.example.com -InstallAx
#   .\scripts\ensure-cloudflare-tunnel.ps1 -Hostname ax.example.com -ConfigPath C:\path\to\config.yml -Port 7070
#
# Requires Administrator to install cloudflared into Program Files and start the Windows service.
# A downloaded cloudflared.exe is installed only when its Authenticode signature is valid and
# signed by Cloudflare, Inc.
param(
    [Parameter(Mandatory = $true)]
    [string]$Hostname,
    [int]$Port = 7070,
    [string]$ConfigPath = '',
    [string]$CloudflaredDir = 'C:\Program Files (x86)\cloudflared',
    [switch]$InstallAx,
    [switch]$SkipService,
    [switch]$SkipAxWeb
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Test-IsAdmin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = @($machine, $user, $env:Path) -join ';'
}

function Get-CloudflaredServiceConfig {
    $query = sc.exe qc Cloudflared 2>$null
    if ($LASTEXITCODE -ne 0) { return $null }
    $line = $query | Where-Object { $_ -match 'BINARY_PATH_NAME' } | Select-Object -First 1
    if (-not $line) { return $null }
    if ($line -match '--config\s+"([^"]+)"') {
        return [PSCustomObject]@{
            BinaryPath = $line
            ConfigPath = $Matches[1]
        }
    }
    return [PSCustomObject]@{
        BinaryPath = $line
        ConfigPath = $null
    }
}

function Resolve-ConfigPath {
    if ($ConfigPath) { return $ConfigPath }
    $svc = Get-CloudflaredServiceConfig
    if ($svc -and $svc.ConfigPath -and (Test-Path $svc.ConfigPath)) {
        return $svc.ConfigPath
    }
    $candidates = @(
        (Join-Path $env:USERPROFILE '_nextcloud\cloudflared\config.yml')
        (Join-Path $env:USERPROFILE '.cloudflared\config.yml')
    )
    foreach ($path in $candidates) {
        if (Test-Path $path) { return $path }
    }
    throw "cloudflared config.yml not found. Pass -ConfigPath or install the Cloudflared service first."
}

function Assert-CloudflareSigned {
    param([string]$Path)
    $signature = Get-AuthenticodeSignature -FilePath $Path
    $subject = if ($signature.SignerCertificate) { [string]$signature.SignerCertificate.Subject } else { '' }
    if ([string]$signature.Status -ne 'Valid' -or $subject -notmatch '(^|,\s*)O="?Cloudflare, Inc\.') {
        throw "Refusing to install $Path`: signature status '$($signature.Status)', signer '$subject' (expected a valid signature by Cloudflare, Inc.)."
    }
}

function Ensure-CloudflaredCli {
    Refresh-Path
    $cmd = Get-Command cloudflared -ErrorAction SilentlyContinue
    if ($cmd) {
        Write-Host "cloudflared: $($cmd.Source)" -ForegroundColor Green
        return $cmd.Source
    }

    $installed = Join-Path $CloudflaredDir 'cloudflared.exe'
    if (Test-Path $installed) {
        Write-Host "cloudflared: $installed" -ForegroundColor Green
        return $installed
    }

    if (-not (Test-IsAdmin)) {
        throw "cloudflared is not installed. Re-run this script as Administrator to install it."
    }

    Write-Step 'Installing cloudflared'
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        winget install --id Cloudflare.cloudflared -e --accept-package-agreements --accept-source-agreements
        Refresh-Path
        $cmd = Get-Command cloudflared -ErrorAction SilentlyContinue
        if ($cmd) {
            Write-Host "Installed via winget: $($cmd.Source)" -ForegroundColor Green
            return $cmd.Source
        }
    }

    New-Item -ItemType Directory -Force -Path $CloudflaredDir | Out-Null
    # The x64 build; Windows on Arm runs it under x64 emulation.
    $url = 'https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe'
    $dest = Join-Path $CloudflaredDir 'cloudflared.exe'
    $download = "$dest.download"
    Write-Host "Downloading $url"
    try {
        Invoke-WebRequest -Uri $url -OutFile $download -TimeoutSec 120 -UseBasicParsing
        if (-not (Test-Path $download)) {
            throw "cloudflared download failed"
        }
        Assert-CloudflareSigned -Path $download
        Move-Item -LiteralPath $download -Destination $dest -Force
    } finally {
        if (Test-Path -LiteralPath $download) { Remove-Item -LiteralPath $download -Force }
    }
    Write-Host "Installed: $dest" -ForegroundColor Green
    return $dest
}

function Ensure-IngressRule {
    param([string]$Path, [string]$TunnelHost, [int]$ServicePort)

    if (-not (Test-Path $Path)) {
        throw "Config not found: $Path"
    }

    $text = [IO.File]::ReadAllText($Path)
    if ($text -match "(?m)^\s*-\s*hostname:\s*$([regex]::Escape($TunnelHost))\s*$") {
        Write-Host "Ingress already configured for $TunnelHost in $Path" -ForegroundColor Green
        return
    }

    Write-Step "Adding ingress for $TunnelHost -> http://localhost:$ServicePort"
    $rule = @(
        "  - hostname: $TunnelHost"
        "    service: http://localhost:$ServicePort"
    ) -join "`n"

    if ($text -match '(?m)^(\s*-\s*service:\s*http_status:404\s*)$') {
        $updated = [regex]::Replace($text, '(?m)^(\s*-\s*service:\s*http_status:404\s*)$', "$rule`n`$1", 1)
    } else {
        $updated = $text.TrimEnd() + "`n$rule`n"
    }

    $utf8 = New-Object System.Text.UTF8Encoding $false
    [IO.File]::WriteAllText($Path, $updated, $utf8)
    Write-Host "Updated $Path" -ForegroundColor Green
}

function Ensure-AxCli {
    Refresh-Path
    if (Get-Command ax -ErrorAction SilentlyContinue) {
        Write-Host "ax CLI: $((Get-Command ax).Source)" -ForegroundColor Green
        return
    }

    Write-Step 'Installing ax CLI'
    $cargoToml = Join-Path $root 'crates\ax-cli\Cargo.toml'
    if (Test-Path $cargoToml) {
        Push-Location $root
        $env:CARGO_TARGET_DIR = 'target-dev'
        cargo install --path crates/ax-cli --force
        if ($LASTEXITCODE -ne 0) { throw 'cargo install ax-cli failed' }
        Pop-Location
    } else {
        # Never evaluate downloaded code as Administrator: the user installs ax and re-runs.
        throw "ax CLI is not installed. Install it first (see https://getax.wenneker.io/), then re-run this script."
    }

    Refresh-Path
    if (-not (Get-Command ax -ErrorAction SilentlyContinue)) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
        if (Test-Path (Join-Path $cargoBin 'ax.exe')) {
            $env:Path = "$cargoBin;$env:Path"
        }
    }
    if (-not (Get-Command ax -ErrorAction SilentlyContinue)) {
        throw 'ax CLI install finished but ax is still not on PATH. Open a new shell or add ~/.cargo/bin to PATH.'
    }
    Write-Host "ax CLI: $((Get-Command ax).Source)" -ForegroundColor Green
}

function Test-PortListening {
    param([int]$ListenPort)
    $matches = netstat -ano | Select-String ":$ListenPort\s" | Select-String 'LISTENING'
    return $null -ne $matches
}

function Ensure-AxWeb {
    param([int]$ListenPort)

    if (Test-PortListening -ListenPort $ListenPort) {
        Write-Host "Command Center already listening on port $ListenPort" -ForegroundColor Green
        return
    }

    Ensure-AxCli
    Write-Step "Starting ax ship --watch on port $ListenPort"
    $ax = (Get-Command ax).Source
    Start-Process -FilePath $ax -ArgumentList @('ship', '--watch', '--port', "$ListenPort") -WindowStyle Minimized
    Start-Sleep -Seconds 3
    if (-not (Test-PortListening -ListenPort $ListenPort)) {
        throw "ax web did not start on port $ListenPort. Run manually: ax ship --watch --port $ListenPort"
    }
    Write-Host "Command Center listening on http://127.0.0.1:$ListenPort" -ForegroundColor Green
}

function Ensure-CloudflaredService {
    param([string]$Exe, [string]$Path)

    if ($SkipService) {
        Write-Host 'Skipping Cloudflared service (manual tunnel mode).' -ForegroundColor Yellow
        return
    }

    $service = Get-Service -Name Cloudflared -ErrorAction SilentlyContinue
    if (-not $service) {
        if (-not (Test-IsAdmin)) {
            throw "Cloudflared Windows service is not installed. Re-run as Administrator to create it."
        }
        Write-Step 'Installing Cloudflared Windows service'
        & $Exe service install --config $Path
        if ($LASTEXITCODE -ne 0) {
            throw "cloudflared service install failed (exit $LASTEXITCODE)"
        }
        $service = Get-Service -Name Cloudflared -ErrorAction SilentlyContinue
    }

    if ($service.Status -ne 'Running') {
        if (-not (Test-IsAdmin)) {
            throw "Cloudflared service is stopped. Re-run as Administrator: Start-Service Cloudflared"
        }
        Write-Step 'Starting Cloudflared service'
        Set-Service -Name Cloudflared -StartupType Automatic -ErrorAction SilentlyContinue
        Start-Service -Name Cloudflared
    } elseif ($service.StartType -ne 'Automatic') {
        if (Test-IsAdmin) {
            Set-Service -Name Cloudflared -StartupType Automatic -ErrorAction SilentlyContinue
        } else {
            Write-Host 'Warning: Cloudflared StartType is not Automatic — re-run as Administrator to persist after reboot.' -ForegroundColor Yellow
        }
    }

    $service = Get-Service -Name Cloudflared
    Write-Host "Cloudflared service: $($service.Status)" -ForegroundColor Green
}

Write-Step "Ensure tunnel for https://$Hostname/ -> http://localhost:$Port"
$config = Resolve-ConfigPath
Write-Host "Config: $config"

if ($InstallAx -and -not $SkipAxWeb) {
    Ensure-AxWeb -ListenPort $Port
} elseif (-not (Test-PortListening -ListenPort $Port)) {
    Write-Host "Warning: nothing listening on port $Port. Start Command Center or pass -InstallAx." -ForegroundColor Yellow
}

$cloudflared = Ensure-CloudflaredCli
Ensure-IngressRule -Path $config -TunnelHost $Hostname -ServicePort $Port
Ensure-CloudflaredService -Exe $cloudflared -Path $config

Write-Host ""
Write-Host "Tunnel ready: https://$Hostname/" -ForegroundColor Green
Write-Host "Local:        http://127.0.0.1:$Port" -ForegroundColor Green
