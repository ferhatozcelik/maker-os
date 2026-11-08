# MakerOS microSD yazıcı — Windows
# MakerOS microSD flasher — Windows
#
#   PowerShell'de / in PowerShell:
#   irm https://raw.githubusercontent.com/ferhatozcelik/maker-os/main/flash.ps1 | iex
#
#   ya da flash.cmd dosyasına çift tıklayın / or double-click flash.cmd
#
# Seçenekler (dosyadan çalıştırırken) / options (when run from a file):
#   -Image <dosya>   yerel .img ya da .img.xz        -Disk <numara>   hedef disk numarası
#   -Version <sürüm> ör. v1.0.0 (varsayılan: en son) -NoWifi          Wi-Fi sorusunu atla
#
# Yalnız USB / SD bağlantılı diskler listelenir; yazmadan önce disk adını elle yazmanız istenir.
# Seçilen diskteki her şey silinir.
param([string]$Image = "", [int]$Disk = -1, [string]$Version = "latest", [switch]$NoWifi)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Repo = "ferhatozcelik/maker-os"
$RawUrl = "https://raw.githubusercontent.com/$Repo/main/flash.ps1"
# xz (resmî Windows derlemesi, tukaani-project) — özet sabitlenmiştir.
$XzUrl = "https://github.com/tukaani-project/xz/releases/download/v5.8.4/xz-5.8.4-windows.zip"
$XzSha = "f31af7638391ecf286d48bc8555ce6e131691a1c52a9e44303bce57069e9de56"

$TR = (Get-Culture).Name -like "tr*"
function T([string]$tr, [string]$en) { if ($TR) { $tr } else { $en } }
function Step([string]$tr, [string]$en) { Write-Host ""; Write-Host "==> " -ForegroundColor Yellow -NoNewline; Write-Host (T $tr $en) -ForegroundColor White }
function Fail([string]$tr, [string]$en) { Write-Host ""; Write-Host ((T "Hata: " "Error: ") + (T $tr $en)) -ForegroundColor Red; Read-Host (T "Kapatmak için Enter" "Press Enter to close") | Out-Null; exit 1 }

# --- Yönetici yetkisi ------------------------------------------------------------------
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Write-Host (T "Yönetici izni isteniyor..." "Requesting administrator permission...")
    if ($PSCommandPath) {
        $a = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$PSCommandPath`"")
        if ($Image) { $a += @("-Image", "`"$Image`"") }
        if ($Disk -ge 0) { $a += @("-Disk", "$Disk") }
        if ($Version -ne "latest") { $a += @("-Version", $Version) }
        if ($NoWifi) { $a += "-NoWifi" }
    } else {
        $a = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "irm $RawUrl | iex")
    }
    Start-Process powershell.exe -Verb RunAs -ArgumentList $a
    exit
}

$Host.UI.RawUI.WindowTitle = "MakerOS flasher"
Write-Host ""
Write-Host "  MakerOS" -ForegroundColor DarkYellow -NoNewline
Write-Host (" — " + (T "microSD yazıcı" "microSD flasher"))

$Cache = Join-Path $env:LOCALAPPDATA "MakerOS"
New-Item -ItemType Directory -Force -Path $Cache | Out-Null

function Get-Sha256([string]$p) { (Get-FileHash -Algorithm SHA256 -Path $p).Hash.ToLowerInvariant() }
function Download([string]$url, [string]$out) {
    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($curl) { & $curl.Source -fL --progress-bar -o $out $url; if ($LASTEXITCODE -ne 0) { Fail "İndirme başarısız: $url" "Download failed: $url" } }
    else { Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $out }
}

# --- 1. İmaj ----------------------------------------------------------------------------
if (-not $Image) {
    Step "En son sürüm aranıyor" "Looking up the release"
    $api = if ($Version -eq "latest") { "https://api.github.com/repos/$Repo/releases/latest" } else { "https://api.github.com/repos/$Repo/releases/tags/$Version" }
    try { $rel = Invoke-RestMethod -UseBasicParsing -Uri $api -Headers @{ "User-Agent" = "makeros-flash" } } catch { Fail "GitHub'a ulaşılamadı." "Could not reach GitHub." }
    $asset = $rel.assets | Where-Object { $_.name -like "*.img.xz" } | Select-Object -First 1
    if (-not $asset) { Fail "Sürümde .img.xz dosyası yok." "No .img.xz asset in the release." }
    Write-Host ((T "Sürüm: " "Release: ") + $asset.name)
    $Image = Join-Path $Cache $asset.name
    $expected = ((Invoke-WebRequest -UseBasicParsing -Uri ($asset.browser_download_url + ".sha256")).Content -split "\s+")[0].Trim().ToLowerInvariant()
    if ((Test-Path $Image) -and ((Get-Sha256 $Image) -eq $expected)) {
        Write-Host (T "Önceden indirilmiş imaj kullanılıyor: $Image" "Using previously downloaded image: $Image")
    } else {
        Step "İmaj indiriliyor" "Downloading image"
        Download $asset.browser_download_url $Image
        Step "İmaj doğrulanıyor (SHA-256)" "Verifying image (SHA-256)"
        if ((Get-Sha256 $Image) -ne $expected) { Remove-Item $Image -Force; Fail "Özet uyuşmadı; dosya silindi, yeniden deneyin." "Checksum mismatch; file removed, try again." }
        Write-Host (T "Doğrulandı." "Verified.")
    }
}
if (-not (Test-Path $Image)) { Fail "İmaj bulunamadı: $Image" "Image not found: $Image" }
$Image = (Resolve-Path $Image).Path

$xz = $null
if ($Image -like "*.xz") {
    $xz = Join-Path $Cache "xz\bin_x86-64\xz.exe"
    if (-not (Test-Path $xz)) {
        Step "xz açıcı indiriliyor" "Downloading the xz decompressor"
        $zip = Join-Path $Cache "xz.zip"
        Download $XzUrl $zip
        if ((Get-Sha256 $zip) -ne $XzSha) { Remove-Item $zip -Force; Fail "xz paketinin özeti uyuşmadı." "xz package checksum mismatch." }
        Expand-Archive -Force -Path $zip -DestinationPath (Join-Path $Cache "xz")
        Remove-Item $zip -Force
    }
}

# --- 2. Disk seçimi ---------------------------------------------------------------------
function Get-Targets {
    Get-Disk | Where-Object { $_.BusType -in @("USB", "SD", "MMC") -and -not $_.IsBoot -and -not $_.IsSystem -and $_.Size -gt 0 } | Sort-Object Number
}
function Human([double]$b) { $u = "B", "KB", "MB", "GB", "TB"; $i = 0; while ($b -ge 1000 -and $i -lt 4) { $b /= 1000; $i++ }; "{0:N1} {1}" -f $b, $u[$i] }

if ($Disk -lt 0) {
    Step "Harici diskler aranıyor" "Looking for external disks"
    $targets = @(Get-Targets)
    while ($targets.Count -eq 0) {
        $r = Read-Host (T "microSD kartı takın ve Enter'a basın (çıkmak için q)" "Insert the microSD card and press Enter (q to quit)")
        if ($r -eq "q") { exit 1 }
        $targets = @(Get-Targets)
    }
    $i = 0
    foreach ($d in $targets) { $i++; Write-Host ("  {0}) disk{1,-4} {2,10}  {3}" -f $i, $d.Number, (Human $d.Size), $d.FriendlyName) }
    $n = 1
    if ($targets.Count -gt 1) { $n = [int](Read-Host (T "Numara seçin [1-$($targets.Count)]" "Choose a number [1-$($targets.Count)]")) }
    if ($n -lt 1 -or $n -gt $targets.Count) { Fail "Geçersiz seçim." "Invalid choice." }
    $Disk = $targets[$n - 1].Number
}
$target = Get-Disk -Number $Disk -ErrorAction SilentlyContinue
if (-not $target) { Fail "Disk yok: $Disk" "No such disk: $Disk" }
if ($target.IsBoot -or $target.IsSystem) { Fail "Sistem diskine yazılamaz." "Refusing to write to the system disk." }

# --- 3. Onay ----------------------------------------------------------------------------
Write-Host ""
Write-Host (T "UYARI: disk$Disk ($($target.FriendlyName), $(Human $target.Size)) üzerindeki HER ŞEY silinecek." "WARNING: EVERYTHING on disk$Disk ($($target.FriendlyName), $(Human $target.Size)) will be erased.") -ForegroundColor Red
$c = Read-Host (T "Onaylamak için disk adını yazın (disk$Disk)" "Type the disk name to confirm (disk$Disk)")
if ($c.Trim() -ne "disk$Disk") { Fail "Onaylanmadı; hiçbir şey yazılmadı." "Not confirmed; nothing was written." }

# --- 4. Wi-Fi (isteğe bağlı) ------------------------------------------------------------
$Ssid = ""; $Psk = ""
if (-not $NoWifi) {
    $Ssid = Read-Host (T "Kartın Wi-Fi'a kendiliğinden bağlanması için ağ adı (boş bırakılabilir)" "Wi-Fi network name for the board (optional, leave empty to skip)")
    if ($Ssid) {
        $sec = Read-Host (T "Wi-Fi parolası" "Wi-Fi password") -AsSecureString
        $Psk = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($sec))
    }
}

# --- 5. Yazma ---------------------------------------------------------------------------
Add-Type -Namespace MakerOS -Name Raw -MemberDefinition @"
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr sa, uint disposition, uint flags, IntPtr template);
"@

Step "Disk hazırlanıyor" "Preparing disk"
try { Clear-Disk -Number $Disk -RemoveData -RemoveOEM -Confirm:$false -ErrorAction Stop } catch { }
try { Set-Disk -Number $Disk -IsOffline $true -ErrorAction Stop; $offline = $true } catch { $offline = $false }

Step "Yazılıyor: disk$Disk" "Writing to disk$Disk"
$sector = [Math]::Max(512, [int]$target.LogicalSectorSize)
$total = [long]0
if ($xz) {
    $robot = & $xz --robot --list $Image 2>$null | Where-Object { $_ -like "totals*" }
    if ($robot) { $total = [long](($robot -split "\s+")[4]) }
    $psi = New-Object Diagnostics.ProcessStartInfo $xz, "-dc `"$Image`""
    $psi.UseShellExecute = $false; $psi.RedirectStandardOutput = $true; $psi.CreateNoWindow = $true
    $proc = [Diagnostics.Process]::Start($psi)
    $src = $proc.StandardOutput.BaseStream
} else {
    $src = [IO.File]::OpenRead($Image); $total = $src.Length; $proc = $null
}

# GENERIC_WRITE, FILE_SHARE_READ|WRITE, OPEN_EXISTING, FILE_FLAG_WRITE_THROUGH|NO_BUFFERING
$h = [MakerOS.Raw]::CreateFile("\\.\PhysicalDrive$Disk", 0x40000000, 3, [IntPtr]::Zero, 3, 0xA0000000, [IntPtr]::Zero)
if ($h.IsInvalid) { Fail "Diske erişilemedi (kod $([Runtime.InteropServices.Marshal]::GetLastWin32Error())). Kartı çıkarıp yeniden takın." "Could not open the disk (code $([Runtime.InteropServices.Marshal]::GetLastWin32Error())). Re-insert the card and try again." }
$dst = New-Object IO.FileStream $h, ([IO.FileAccess]::Write), 0, $false

$buf = New-Object byte[] (4MB)
$written = [long]0
$sw = [Diagnostics.Stopwatch]::StartNew()
$last = 0
try {
    while ($true) {
        $fill = 0
        while ($fill -lt $buf.Length) { $r = $src.Read($buf, $fill, $buf.Length - $fill); if ($r -le 0) { break }; $fill += $r }
        if ($fill -eq 0) { break }
        if ($fill % $sector -ne 0) { $pad = $sector - ($fill % $sector); [Array]::Clear($buf, $fill, $pad); $fill += $pad }
        $dst.Write($buf, 0, $fill)
        $written += $fill
        if ($sw.ElapsedMilliseconds - $last -ge 1000) {
            $last = $sw.ElapsedMilliseconds
            $mbs = [Math]::Round($written / 1MB / [Math]::Max(1, $sw.Elapsed.TotalSeconds), 1)
            $pct = if ($total -gt 0) { [Math]::Min(100, [int](100 * $written / $total)) } else { -1 }
            $ProgressPreference = "Continue"
            Write-Progress -Activity (T "MakerOS yazılıyor" "Writing MakerOS") -Status ("{0} / {1}  ({2} MB/s)" -f (Human $written), (Human $total), $mbs) -PercentComplete $pct
            $ProgressPreference = "SilentlyContinue"
        }
        if ($fill -lt $buf.Length) { break }
    }
    $dst.Flush()
} catch {
    Fail ("Yazma başarısız: " + $_.Exception.Message) ("Write failed: " + $_.Exception.Message)
} finally {
    $dst.Dispose(); $src.Dispose(); if ($proc) { $proc.WaitForExit() }
    $ProgressPreference = "Continue"; Write-Progress -Activity "MakerOS" -Completed; $ProgressPreference = "SilentlyContinue"
}
if ($proc -and $proc.ExitCode -ne 0) { Fail "İmaj açılamadı (xz)." "Could not decompress the image (xz)." }
Write-Host ((T "Yazıldı: " "Written: ") + (Human $written) + " · " + [int]$sw.Elapsed.TotalSeconds + " s")

if ($offline) { try { Set-Disk -Number $Disk -IsOffline $false } catch { } }
try { Update-Disk -Number $Disk } catch { }

# --- 6. Wi-Fi dosyası -------------------------------------------------------------------
if ($Ssid) {
    Step "Wi-Fi ayarı yazılıyor" "Writing Wi-Fi settings"
    $root = $null
    for ($k = 0; $k -lt 15 -and -not $root; $k++) {
        Start-Sleep -Seconds 1
        $p = Get-Partition -DiskNumber $Disk -ErrorAction SilentlyContinue | Where-Object { ($_ | Get-Volume -ErrorAction SilentlyContinue).FileSystemLabel -eq "MOSCFG" } | Select-Object -First 1
        if ($p) {
            if (-not $p.DriveLetter -or $p.DriveLetter -eq [char]0) { try { $p | Add-PartitionAccessPath -AssignDriveLetter -ErrorAction Stop; $p = Get-Partition -DiskNumber $Disk -PartitionNumber $p.PartitionNumber } catch { } }
            if ($p.DriveLetter -and $p.DriveLetter -ne [char]0) { $root = "$($p.DriveLetter):\" }
        }
    }
    if ($root) {
        [IO.File]::WriteAllText((Join-Path $root "wifi.txt"), "SSID=$Ssid`nPAROLA=$Psk`n", (New-Object Text.UTF8Encoding $false))
        Write-Host ((T "wifi.txt yazıldı: " "wifi.txt written: ") + $root)
    } else {
        Write-Host (T "MOSCFG bölümü açılamadı; wifi.txt'yi elle ekleyin." "Could not open MOSCFG; add wifi.txt manually.") -ForegroundColor Yellow
    }
}

Write-Host ""
Write-Host ("✓ " + (T "Hazır! Kartı çıkarıp cihaza takın ve gücü verin." "Done! Remove the card, insert it into the board and power it on.")) -ForegroundColor Green
Write-Host (T "Ardından: http://maker-os.local/  (ağ yoksa `"MakerOS`" Wi-Fi'ına bağlanın)" "Then open: http://maker-os.local/  (no network? join the `"MakerOS`" Wi-Fi)")
Write-Host (T "Windows kartı biçimlendirmek isterse 'İptal'e basın." "If Windows offers to format the card, click 'Cancel'.") -ForegroundColor Yellow
Read-Host (T "Kapatmak için Enter" "Press Enter to close") | Out-Null
