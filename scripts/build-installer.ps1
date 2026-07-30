param(
    [string]$Configuration = "release",
    [string]$VpkPath = "",
    [string]$PackId = "Hayai",
    [string]$PackTitle = "Hayai",
    [string]$PackAuthors = "Hayai",
    [switch]$SkipBuild,
    [switch]$KeepStage
)

$ErrorActionPreference = "Stop"

$projectRoot = [System.IO.Path]::GetDirectoryName($PSScriptRoot)
$cargoToml = Join-Path $projectRoot "Cargo.toml"
$buildOutput = Join-Path $projectRoot "target\\$Configuration"
$distDir = Join-Path $projectRoot "dist"
$stageDir = Join-Path $distDir "velopack-stage"
$outputDir = Join-Path $distDir "velopack"

function Read-CargoValue {
    param(
        [Parameter(Mandatory = $true)][string]$LiteralPath,
        [Parameter(Mandatory = $true)][string]$Key
    )

    $line = Get-Content -LiteralPath $LiteralPath | Where-Object { $_ -match "^$Key\s*=\s*`"(.+)`"" } | Select-Object -First 1
    if (-not $line) {
        throw "Unable to read '$Key' from $LiteralPath"
    }

    return [regex]::Match($line, "^$Key\s*=\s*`"(.+)`"").Groups[1].Value
}

function Resolve-Vpk {
    param([string]$ExplicitPath)

    if ($ExplicitPath) {
        if (-not (Test-Path -LiteralPath $ExplicitPath)) {
            throw "Velopack CLI not found: $ExplicitPath"
        }
        return $ExplicitPath
    }

    $command = Get-Command vpk -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if (-not $dotnet) {
        throw "Velopack CLI not found. Install the .NET SDK and then run: dotnet tool install -g vpk"
    }

    throw "Velopack CLI not found. Run: dotnet tool install -g vpk"
}

function New-CleanDirectory {
    param([Parameter(Mandatory = $true)][string]$LiteralPath)

    if (Test-Path -LiteralPath $LiteralPath) {
        Remove-Item -LiteralPath $LiteralPath -Recurse -Force
    }
    New-Item -ItemType Directory -Path $LiteralPath -Force | Out-Null
}

Push-Location -LiteralPath $projectRoot
try {
    if (-not $SkipBuild) {
        if ($Configuration -eq "release") {
            cargo build --release
        }
        else {
            cargo build
        }
        if ($LASTEXITCODE -ne 0) {
            throw "Cargo build failed with exit code $LASTEXITCODE"
        }
    }

    $packageName = Read-CargoValue -LiteralPath $cargoToml -Key "name"
    $version = Read-CargoValue -LiteralPath $cargoToml -Key "version"
    $exeName = "$packageName.exe"
    $exePath = Join-Path $buildOutput $exeName

    if (-not (Test-Path -LiteralPath $exePath)) {
        throw "Build output not found: $exePath"
    }

    $vpk = Resolve-Vpk -ExplicitPath $VpkPath

    New-Item -ItemType Directory -Path $distDir -Force | Out-Null
    New-CleanDirectory -LiteralPath $stageDir
    New-CleanDirectory -LiteralPath $outputDir

    Copy-Item -LiteralPath $exePath -Destination (Join-Path $stageDir $exeName) -Force

    $runtimeFiles = Get-ChildItem -LiteralPath $buildOutput -File | Where-Object {
        $_.Extension -ieq ".dll"
    }
    foreach ($file in $runtimeFiles) {
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $stageDir $file.Name) -Force
    }

    $iconPath = Join-Path $projectRoot "assets\\hayai.ico"
    $arguments = @(
        "pack",
        "--packId", $PackId,
        "--packVersion", $version,
        "--packDir", $stageDir,
        "--mainExe", $exeName,
        "--packTitle", $PackTitle,
        "--packAuthors", $PackAuthors,
        "--outputDir", $outputDir,
        "--delta", "None",
        "--shortcuts", "Desktop,StartMenuRoot"
    )
    if (Test-Path -LiteralPath $iconPath) {
        $arguments += @("--icon", $iconPath)
    }

    Write-Host "Using Velopack CLI: $vpk"
    Write-Host "Packaging staged payload from $stageDir"
    & $vpk @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Velopack packaging failed with exit code $LASTEXITCODE"
    }

    if (-not $KeepStage) {
        Remove-Item -LiteralPath $stageDir -Recurse -Force
    }

    Write-Host "Velopack installer output written to $outputDir"
}
finally {
    Pop-Location
}
