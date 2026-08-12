param(
    [string]$Configuration = "release",
    [string]$VpkPath = "",
    [string]$PackId = "Hayai",
    [string]$PackTitle = "Hayai",
    [string]$PackAuthors = "Hayai",
    [string]$ScryPackage = "",
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

function Resolve-ScryPackage {
    param(
        [Parameter(Mandatory = $true)][string]$LockPath,
        [string]$ExplicitPackage
    )

    $lock = Get-Content -LiteralPath $LockPath -Raw
    $match = [regex]::Match(
        $lock,
        '(?ms)^name = "scry-client"\s+version = "[^"]+"\s+source = "git\+https://github\.com/Ariaryy/scry-search\?tag=(?<tag>[^#"]+)#[0-9a-f]{40}"'
    )
    if (-not $match.Success) {
        throw "Unable to resolve the tagged Scry Search release from $LockPath"
    }

    $tag = $match.Groups["tag"].Value
    $cacheDir = Join-Path $projectRoot "target\scry-release"
    New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null

    if ($ExplicitPackage) {
        $archive = [System.IO.Path]::GetFullPath((Join-Path $projectRoot $ExplicitPackage))
        if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
            throw "ScryPackage archive not found: $archive"
        }
    }
    else {
        $archiveName = "scry-search-$tag-windows-x86_64.zip"
        $archive = Join-Path $cacheDir $archiveName
        if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
            $url = "https://github.com/Ariaryy/scry-search/releases/download/$tag/$archiveName"
            Write-Host "Downloading first-party Scry Search release $tag"
            Invoke-WebRequest -Uri $url -OutFile $archive
        }
    }

    $root = Join-Path $cacheDir $tag
    if (-not (Test-Path -LiteralPath (Join-Path $root "scryd.exe"))) {
        if (Test-Path -LiteralPath $root) {
            Remove-Item -LiteralPath $root -Recurse -Force
        }
        Expand-Archive -LiteralPath $archive -DestinationPath $root
    }
    foreach ($required in @("scryd.exe", "scry.exe", "install-daemon.ps1", "uninstall-daemon.ps1")) {
        if (-not (Test-Path -LiteralPath (Join-Path $root $required) -PathType Leaf)) {
            throw "Scry Search release $tag is missing $required"
        }
    }
    return $root
}

Push-Location -LiteralPath $projectRoot
try {
    $scryPackageRoot = Resolve-ScryPackage -LockPath (Join-Path $projectRoot "Cargo.lock") -ExplicitPackage $ScryPackage
    if (-not $SkipBuild) {
        if ($Configuration -eq "release") {
            cargo build --locked --release
        }
        else {
            cargo build --locked
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

    foreach ($file in @("scryd.exe", "scry.exe", "install-daemon.ps1", "uninstall-daemon.ps1")) {
        Copy-Item -LiteralPath (Join-Path $scryPackageRoot $file) -Destination $stageDir -Force
    }

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
