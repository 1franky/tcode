# Instalador de tcode para Windows (PLAN.md §10): descarga el .zip de la
# última release de GitHub (o la que indique $env:TCODE_VERSION, p. ej.
# "v0.1.0"), lo extrae en modo portable y opcionalmente añade la carpeta
# al PATH del USUARIO (no del sistema — sin necesidad de elevación).
#
# Uso (PowerShell):
#   irm https://raw.githubusercontent.com/1franky/tcode/main/install/windows.ps1 | iex
#
# Por defecto instala en modo portable en
# $env:LOCALAPPDATA\tcode (sin admin, sin tocar el registro).

$ErrorActionPreference = "Stop"

$Repo = "1franky/tcode"
$DestinoDir = Join-Path $env:LOCALAPPDATA "tcode"
$Version = if ($env:TCODE_VERSION) { $env:TCODE_VERSION } else { "latest" }

function Write-Log($Mensaje) {
    Write-Host "==> $Mensaje" -ForegroundColor Cyan
}

function Write-ErrorAndExit($Mensaje) {
    Write-Host "error: $Mensaje" -ForegroundColor Red
    exit 1
}

if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64") {
    Write-ErrorAndExit "Solo hay binario pre-compilado para Windows x86_64 por ahora (detectado: $env:PROCESSOR_ARCHITECTURE)."
}

$NombreAsset = "tcode-windows-x86_64.zip"
if ($Version -eq "latest") {
    $UrlDescarga = "https://github.com/$Repo/releases/latest/download/$NombreAsset"
} else {
    $UrlDescarga = "https://github.com/$Repo/releases/download/$Version/$NombreAsset"
}

Write-Log "Descargando tcode (windows-x86_64, $Version)..."
$ArchivoTmp = Join-Path $env:TEMP "tcode-install.zip"
try {
    Invoke-WebRequest -Uri $UrlDescarga -OutFile $ArchivoTmp -UseBasicParsing
} catch {
    Write-ErrorAndExit "no se pudo descargar $UrlDescarga - ¿ya existe una release publicada? (github.com/$Repo/releases)"
}

Write-Log "Instalando en $DestinoDir..."
New-Item -ItemType Directory -Force -Path $DestinoDir | Out-Null
Expand-Archive -Path $ArchivoTmp -DestinationPath $DestinoDir -Force
Remove-Item $ArchivoTmp

# El .exe no está firmado (Authenticode): Windows SmartScreen puede
# avisar "Windows protegió su PC" la primera vez. "Más información" ->
# "Ejecutar de todas formas" lo permite; esto no aplica el bloqueo de
# forma persistente, solo informa.
Unblock-File -Path (Join-Path $DestinoDir "tcode.exe") -ErrorAction SilentlyContinue

$PathUsuario = [Environment]::GetEnvironmentVariable("Path", "User")
if ($PathUsuario -notlike "*$DestinoDir*") {
    Write-Log "Añadiendo $DestinoDir al PATH del usuario..."
    [Environment]::SetEnvironmentVariable("Path", "$PathUsuario;$DestinoDir", "User")
    Write-Log "Abre una terminal nueva para que 'tcode' quede disponible."
} else {
    Write-Log "$DestinoDir ya estaba en el PATH."
}

Write-Log "tcode instalado correctamente. Prueba con: tcode"
