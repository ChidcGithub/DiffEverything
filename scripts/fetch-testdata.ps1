param(
    [string]$OutDir = (Join-Path $PSScriptRoot '..\testdata\external')
)

$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force $OutDir | Out-Null
$manifestPath = Join-Path $OutDir 'MANIFEST.csv'
$manifest = @()
if (Test-Path $manifestPath) {
    $manifest = @(Import-Csv $manifestPath)
}

function Add-ManifestEntry {
    param([string]$FileName, [string]$Url, [string]$Path)
    if ($script:manifest | Where-Object { $_.file -eq $FileName }) {
        return
    }
    $script:manifest += [pscustomobject]@{
        file = $FileName
        url = $Url
        sha256 = (Get-FileHash $Path -Algorithm SHA256).Hash
        bytes = (Get-Item $Path).Length
    }
}

function Get-RemoteFile {
    param([string]$Url, [string]$FileName)
    $dest = Join-Path $OutDir $FileName
    if (Test-Path $dest) {
        Write-Host "[cached] $FileName"
        Add-ManifestEntry $FileName $Url $dest
        return $dest
    }
    $candidates = @($Url)
    if ($Url -match '^https://(raw\.githubusercontent\.com|github\.com)/') {
        $candidates += "https://gh-proxy.com/$Url"
    }
    foreach ($candidate in $candidates) {
        try {
            Write-Host "[download] $candidate"
            Invoke-WebRequest -Uri $candidate -OutFile $dest -UseBasicParsing -TimeoutSec 120 -ErrorAction Stop
            if ((Get-Item $dest).Length -eq 0) { throw 'empty response' }
            Add-ManifestEntry $FileName $Url $dest
            return $dest
        } catch {
            Write-Warning "failed: $candidate -> $($_.Exception.Message)"
            Remove-Item $dest -ErrorAction SilentlyContinue
        }
    }
    Write-Warning "SKIPPED $FileName"
    return $null
}

Get-RemoteFile 'https://www.w3.org/WAI/ER/tests/xhtml/testfiles/resources/pdf/dummy.pdf' 'dummy.pdf' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/mozilla/pdf.js/master/web/compressed.tracemonkey-pldi-09.pdf' 'tracemonkey.pdf' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/python-openxml/python-docx/master/tests/test_files/test.docx' 'python-docx-test.docx' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/python-openxml/python-docx/master/tests/test_files/having-images.docx' 'python-docx-images.docx' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/tafia/calamine/master/tests/any_sheets.xlsx' 'calamine-any-sheets.xlsx' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/tafia/calamine/master/tests/header-row.xlsx' 'calamine-header-row.xlsx' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/tafia/calamine/master/tests/any_sheets.xls' 'calamine-any-sheets.xls' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/tafia/calamine/master/tests/any_sheets.ods' 'calamine-any-sheets.ods' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/plotly/datasets/master/iris.csv' 'iris.csv' | Out-Null
Get-RemoteFile 'https://raw.githubusercontent.com/kubernetes/website/main/content/en/examples/application/nginx-app.yaml' 'nginx-app.yaml' | Out-Null
Get-RemoteFile 'https://www.w3schools.com/xml/note.xml' 'note.xml' | Out-Null
Get-RemoteFile 'https://www.w3schools.com/xml/cd_catalog.xml' 'cd_catalog.xml' | Out-Null
Get-RemoteFile 'https://jsonplaceholder.typicode.com/todos/1' 'todo.json' | Out-Null
Get-RemoteFile 'https://picsum.photos/id/1/300/200.jpg' 'image-a.jpg' | Out-Null
Get-RemoteFile 'https://picsum.photos/id/2/300/200.jpg' 'image-b.jpg' | Out-Null
Get-RemoteFile 'https://ftp.gnu.org/gnu/hello/hello-2.12.tar.gz' 'hello-2.12.tar.gz' | Out-Null
Get-RemoteFile 'https://ftp.debian.org/debian/pool/main/h/hello/hello_2.10-2_amd64.deb' 'hello_2.10-2_amd64.deb' | Out-Null
Get-RemoteFile 'https://ftp.debian.org/debian/pool/main/h/hello/hello_2.10-3_amd64.deb' 'hello_2.10-3_amd64.deb' | Out-Null
Get-RemoteFile 'https://archive.archlinux.org/packages/w/which/which-2.22-1-x86_64.pkg.tar.zst' 'which-2.22-1-x86_64.pkg.tar.zst' | Out-Null
Get-RemoteFile 'https://archive.archlinux.org/packages/w/which/which-2.25-1-x86_64.pkg.tar.zst' 'which-2.25-1-x86_64.pkg.tar.zst' | Out-Null
Get-RemoteFile 'https://archives.fedoraproject.org/pub/archive/fedora/linux/releases/39/Everything/x86_64/os/Packages/h/hello-2.12.1-2.fc39.x86_64.rpm' 'hello-2.12.1-2.fc39.x86_64.rpm' | Out-Null
Get-RemoteFile 'https://archives.fedoraproject.org/pub/archive/fedora/linux/releases/40/Everything/x86_64/os/Packages/h/hello-2.12.1-4.fc40.x86_64.rpm' 'hello-2.12.1-4.fc40.x86_64.rpm' | Out-Null
Get-RemoteFile 'https://vault.centos.org/8.4.2105/BaseOS/x86_64/os/Packages/which-2.21-12.el8.x86_64.rpm' 'which-2.21-12.el8.x86_64.rpm' | Out-Null
Get-RemoteFile 'https://vault.centos.org/8.5.2111/BaseOS/x86_64/os/Packages/which-2.21-16.el8.x86_64.rpm' 'which-2.21-16.el8.x86_64.rpm' | Out-Null
Get-RemoteFile 'https://github.com/kygo8/open-diff/archive/refs/heads/master.zip' 'open-diff-master.zip' | Out-Null

# Portable Tesseract is used by the optional OCR mode.
$tesseractArchive = Get-RemoteFile 'https://github.com/mikhailgarber/tesseract-portable/releases/download/v5.5.3_5/tesseract-5.5.3_5-x64-windows-static.tar.gz' 'tesseract-portable.tar.gz'
if ($tesseractArchive) {
    $tesseractDir = Join-Path $OutDir 'tesseract'
    if (-not (Test-Path (Join-Path $tesseractDir 'tessdata/eng.traineddata'))) {
        Write-Host '[extract] tesseract-portable.tar.gz'
        New-Item -ItemType Directory -Force $tesseractDir | Out-Null
        tar -xzf $tesseractArchive -C $tesseractDir
    }
}

# HDiffPatch provides hdiffz/hpatchz for HDiffPatch-compatible patch handling.
$hdiffZip = Get-RemoteFile 'https://github.com/sisong/HDiffPatch/releases/download/v5.1.3/hdiffpatch_v5.1.3_bin_windows64.zip' 'hdiffpatch_windows64.zip'
if ($hdiffZip) {
    $hdiffDir = Join-Path $OutDir 'hdiffpatch'
    if (-not (Get-ChildItem $hdiffDir -Recurse -Filter hdiffz.exe -ErrorAction SilentlyContinue)) {
        Write-Host '[extract] hdiffpatch_windows64.zip'
        Expand-Archive -Path $hdiffZip -DestinationPath $hdiffDir -Force
    }
}

# Poppler provides pdftoppm, used by the optional PDF visual-diff mode.
$popplerZip = Get-RemoteFile 'https://github.com/oschwartz10612/poppler-windows/releases/download/v24.08.0-0/Release-24.08.0-0.zip' 'poppler-windows.zip'
if ($popplerZip) {
    $popplerDir = Join-Path $OutDir 'poppler'
    if (-not (Test-Path (Join-Path $popplerDir 'Library/bin/pdftoppm.exe'))) {
        Write-Host '[extract] poppler-windows.zip'
        Expand-Archive -Path $popplerZip -DestinationPath $popplerDir -Force
    }
}

$manifest = @($manifest | Sort-Object file -Unique)
$manifest | Export-Csv -Path $manifestPath -NoTypeInformation -Encoding utf8
Write-Host "`nManifest written to $manifestPath"
Get-ChildItem $OutDir -File | Sort-Object Name | Select-Object Name, Length
