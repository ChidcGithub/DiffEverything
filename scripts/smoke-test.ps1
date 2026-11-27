param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\debug\diff-everything.exe')
)

$ErrorActionPreference = 'Continue'
$script:failures = 0
$script:passes = 0

function Invoke-Case {
    param(
        [string]$Name,
        [int]$ExpectedExit,
        [string[]]$Arguments,
        [string]$Contains = ''
    )
    $output = & $Exe @Arguments 2>&1 | Out-String
    $code = $LASTEXITCODE
    $ok = ($code -eq $ExpectedExit)
    if ($Contains -and -not $output.Contains($Contains)) { $ok = $false }
    if ($ok) {
        $script:passes++
        Write-Host "[PASS] $Name"
    } else {
        $script:failures++
        Write-Host "[FAIL] $Name (exit=$code expected=$ExpectedExit)"
        if ($Contains) { Write-Host "       expected substring: $Contains" }
        Write-Host ($output -split "`n" | Select-Object -First 8 | Out-String)
    }
}

$fixtures = Join-Path $PSScriptRoot '..\fixtures'
$external = Join-Path $PSScriptRoot '..\testdata\external'

Invoke-Case 'equal text' 0 @("$fixtures\text\old.txt", "$fixtures\text\old.txt") 'EQUAL'
Invoke-Case 'text diff' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--no-color') '@@ '
Invoke-Case 'head text content' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--no-color', '--head', '2', '--context', '0') 'text and code diff'
Invoke-Case 'last text content' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--no-color', '--last', '2', '--context', '0') 'ZIP archive diff'
Invoke-Case 'filter text content' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--no-color', '--filter', 'JSON semantic', '--context', '0') 'JSON semantic diff'
Invoke-Case 'filter directory path' 1 @("$fixtures\dir\left", "$fixtures\dir\right", '--no-color', '--filter', 'README') 'README.md'
Invoke-Case 'concise text' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--no-color', '--detail', 'concise') '- text and code diff'
Invoke-Case 'json semantic' 1 @("$fixtures\json\old.json", "$fixtures\json\new.json", '--no-color') 'JSON:'
Invoke-Case 'yaml semantic' 1 @("$fixtures\structured\old.yaml", "$fixtures\structured\new.yaml", '--no-color') 'YAML:'
Invoke-Case 'toml semantic' 1 @("$fixtures\structured\old.toml", "$fixtures\structured\new.toml", '--no-color') 'TOML:'
Invoke-Case 'ini semantic' 1 @("$fixtures\structured\old.ini", "$fixtures\structured\new.ini", '--no-color') 'INI:'
Invoke-Case 'xml semantic' 1 @("$fixtures\structured\old.xml", "$fixtures\structured\new.xml", '--no-color') 'XML:'
Invoke-Case 'html semantic' 1 @("$fixtures\structured\old.html", "$fixtures\structured\new.html", '--no-color') 'HTML:'
Invoke-Case 'openapi breaking changes' 1 @("$fixtures\openapi\old.yaml", "$fixtures\openapi\new.yaml", '--no-color') 'breaking'
Invoke-Case 'plist semantic' 1 @("$fixtures\plist\old.plist", "$fixtures\plist\new.plist", '--no-color') 'PLIST:'
Invoke-Case 'plist binary equal' 0 @("$fixtures\plist\old.plist", "$fixtures\plist\old-binary.plist", '--quiet')
Invoke-Case 'pickle semantic' 1 @("$fixtures\pickle\old.pkl", "$fixtures\pickle\new.pkl", '--no-color') 'PICKLE:'
Invoke-Case 'pickle protocol equal' 0 @("$fixtures\pickle\old.pkl", "$fixtures\pickle\old-protocol5.pkl", '--quiet')
Invoke-Case 'pickle globals readable' 1 @("$fixtures\pickle\old-global.pkl", "$fixtures\pickle\new-global.pkl", '--no-color') 'Widget'
Invoke-Case 'sqlite semantic' 1 @("$fixtures\sqlite\old.db", "$fixtures\sqlite\new.db", '--no-color') 'SQLITE:'
Invoke-Case 'sqlite page layout equal' 0 @("$fixtures\sqlite\old.db", "$fixtures\sqlite\old-copy.db", '--no-color') 'EQUAL'
Invoke-Case 'elf semantic' 1 @("$fixtures\executable\hello-old", "$fixtures\executable\hello-new", '--no-color') 'ELF:'
Invoke-Case 'elf byte offsets' 1 @("$fixtures\executable\hello-old", "$fixtures\executable\hello-new", '--no-color', '--detail', 'full') 'byte_diff'
Invoke-Case 'default concise text' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--no-color') '- text and code diff'
Invoke-Case 'macho fat semantic' 1 @("$fixtures\executable\diffoscope-test1.macho", "$fixtures\executable\diffoscope-test1-mod.macho", '--no-color') 'MACH-O-FAT:'
Invoke-Case 'pe fixture semantic' 1 @("$fixtures\executable\diffoscope-test1.exe", "$fixtures\executable\diffoscope-test2.exe", '--no-color') 'PE:'
Invoke-Case 'font semantic' 1 @("$fixtures\font\regular.ttf", "$fixtures\font\bold.ttf", '--no-color') 'FONT:'
Invoke-Case 'font woff semantic' 1 @("$fixtures\font\regular.woff", "$fixtures\font\bold.woff", '--no-color') 'FONT:'
Invoke-Case 'font woff ttf equal' 0 @("$fixtures\font\regular.ttf", "$fixtures\font\regular.woff", '--no-color') 'EQUAL'
Invoke-Case 'font woff2 semantic' 1 @("$fixtures\font\regular.woff2", "$fixtures\font\bold.woff2", '--no-color') 'FONT:'
Invoke-Case 'font woff2 ttf equal' 0 @("$fixtures\font\regular.ttf", "$fixtures\font\regular.woff2", '--no-color') 'EQUAL'
Invoke-Case 'pe semantic' 1 @("$external\hdiffpatch\windows64\hdiffz.exe", "$external\hdiffpatch\windows64\hpatchz.exe", '--no-color') 'PE:'
Invoke-Case 'cab archive' 1 @("$fixtures\cab\old.cab", "$fixtures\cab\new.cab", '--no-color') 'cab archive:'
Invoke-Case 'xar archive' 1 @("$fixtures\xar\left.xar", "$fixtures\xar\right.xar", '--no-color') 'xar archive:'
Invoke-Case 'xar equal' 0 @("$fixtures\xar\left.xar", "$fixtures\xar\left.xar", '--no-color') 'EQUAL'
Invoke-Case 'xar codecs' 1 @("$fixtures\xar\mixed-left.xar", "$fixtures\xar\mixed-right.xar", '--no-color') 'xar archive: 5 entries'
Invoke-Case 'xar symlink' 1 @("$fixtures\xar\links-left.xar", "$fixtures\xar\links-right.xar", '--no-color') 'symlink ->'
Invoke-Case 'ar archive' 1 @("$fixtures\ar\test1.a", "$fixtures\ar\test2.a", '--no-color') 'ar archive:'
Invoke-Case 'bsd ar archive' 1 @("$fixtures\ar\bsd-old.a", "$fixtures\ar\bsd-new.a", '--no-color') 'long_member_name_one.txt text:'
Invoke-Case 'tar symlink' 1 @("$fixtures\archives\links-old.tar", "$fixtures\archives\links-new.tar", '--no-color') 'symlink -> docs/readme.md'
Invoke-Case 'java class semantic' 1 @("$fixtures\java\Test1.class", "$fixtures\java\Test2.class", '--no-color') 'JAVA-CLASS:'
Invoke-Case 'java class equal' 0 @("$fixtures\java\Test1.class", "$fixtures\java\Test1.class", '--no-color') 'EQUAL'
Invoke-Case 'jar java class' 1 @("$fixtures\java\old.jar", "$fixtures\java\new.jar", '--no-color') 'Test.class JAVA-CLASS:'
Invoke-Case 'ssh public key' 1 @("$fixtures\ssh\test_openssh_pub_key1.pub", "$fixtures\ssh\test_openssh_pub_key2.pub", '--no-color') 'OPENSSH-KEY:'
Invoke-Case 'ssh public key equal' 0 @("$fixtures\ssh\test_openssh_pub_key1.pub", "$fixtures\ssh\test_openssh_pub_key1.pub", '--no-color') 'EQUAL'
Invoke-Case 'ipk package' 1 @("$fixtures\ipk\old.ipk", "$fixtures\ipk\new.ipk", '--no-color') 'control.tar.gz tar.gz archive:'
Invoke-Case 'ipk package equal' 0 @("$fixtures\ipk\old.ipk", "$fixtures\ipk\old.ipk", '--no-color') 'EQUAL'
Invoke-Case 'ico semantic' 1 @("$fixtures\images\diffoscope-test1.ico", "$fixtures\images\diffoscope-test2.ico", '--no-color') 'ICO:'
Invoke-Case 'ico equal' 0 @("$fixtures\images\diffoscope-test1.ico", "$fixtures\images\diffoscope-test1.ico", '--no-color') 'EQUAL'
Invoke-Case 'git index' 1 @("$fixtures\git\test1.git-index", "$fixtures\git\test2.git-index", '--no-color') 'GIT-INDEX:'
Invoke-Case 'git index equal' 0 @("$fixtures\git\test1.git-index", "$fixtures\git\test1.git-index", '--no-color') 'EQUAL'
Invoke-Case 'apk manifest' 1 @("$fixtures\apk\old.apk", "$fixtures\apk\new.apk", '--no-color') 'AndroidManifest.xml ANDROID-XML:'
Invoke-Case 'apk manifest equal' 0 @("$fixtures\apk\old.apk", "$fixtures\apk\old.apk", '--no-color') 'EQUAL'
Invoke-Case 'wasm semantic' 1 @("$fixtures\wasm\old.wasm", "$fixtures\wasm\new.wasm", '--no-color') 'WASM:'
Invoke-Case 'wasm equal' 0 @("$fixtures\wasm\old.wasm", "$fixtures\wasm\old.wasm", '--no-color') 'EQUAL'
Invoke-Case 'gettext semantic' 1 @("$fixtures\gettext\test1.mo", "$fixtures\gettext\test2.mo", '--no-color') 'GETTEXT:'
Invoke-Case 'gettext equal' 0 @("$fixtures\gettext\test1.mo", "$fixtures\gettext\test1.mo", '--no-color') 'EQUAL'
Invoke-Case 'gettext plural' 1 @("$fixtures\gettext\plural-old.mo", "$fixtures\gettext\plural-new.mo", '--no-color') '$.messages[0].msgstr[1]'
Invoke-Case 'cross-format json/yaml' 1 @("$fixtures\structured\cross.yaml", "$fixtures\structured\cross.json", '--no-color') 'YAML+JSON'
Invoke-Case 'csv table key' 1 @("$fixtures\table\old.csv", "$fixtures\table\new.csv", '--key', 'id', '--no-color') 'age: 30 -> 31'
Invoke-Case 'tsv table' 1 @("$fixtures\table\old.tsv", "$fixtures\table\new.tsv", '--key', 'id', '--no-color') 'tsv:'
Invoke-Case 'docx text' 1 @("$fixtures\docs\old.docx", "$fixtures\docs\new.docx", '--no-color') 'docx:'
Invoke-Case 'pptx text' 1 @("$fixtures\docs\old.pptx", "$fixtures\docs\new.pptx", '--no-color') 'pptx:'
Invoke-Case 'pdf text' 1 @("$fixtures\pdf\old.pdf", "$fixtures\pdf\new.pdf", '--no-color') 'pdf:'
Invoke-Case 'pdf page markers' 1 @("$fixtures\pdf\old.pdf", "$fixtures\pdf\new.pdf", '--no-color') '# Page 2'
Invoke-Case 'ofd text' 1 @("$fixtures\ofd\old.ofd", "$fixtures\ofd\new.ofd", '--no-color') 'ofd:'
Invoke-Case 'xlsx workbook' 1 @("$fixtures\workbook\old.xlsx", "$fixtures\workbook\new.xlsx", '--no-color') 'xlsx:'
Invoke-Case 'image pixels' 1 @("$fixtures\images\old.png", "$fixtures\images\new.png", '--no-color') 'pixels differ'
Invoke-Case 'binary hex' 1 @("$fixtures\binary\old.bin", "$fixtures\binary\new.bin", '--no-color') 'byte(s) differ'
Invoke-Case 'audio metadata' 1 @("$fixtures\audio\old.wav", "$fixtures\audio\new.wav", '--no-color') 'audio:'
Invoke-Case 'directory recursion' 1 @("$fixtures\dir\left", "$fixtures\dir\right", '--no-color') 'directory:'
Invoke-Case 'zip archive' 1 @("$fixtures\zip\old.zip", "$fixtures\zip\new.zip", '--no-color') 'zip archive:'
Invoke-Case 'zip zstd entry compression' 1 @("$fixtures\zip-methods\old-zstd.zip", "$fixtures\zip-methods\new-zstd.zip", '--no-color') 'zip archive:'
Invoke-Case 'zip lzma entry compression' 1 @("$fixtures\zip-methods\old-lzma.zip", "$fixtures\zip-methods\new-lzma.zip", '--no-color') 'zip archive:'
Invoke-Case 'tar.gz archive' 1 @("$fixtures\archives\old.tar.gz", "$fixtures\archives\new.tar.gz", '--no-color') 'tar.gz archive:'
Invoke-Case 'tar.bz2 archive' 1 @("$fixtures\archives\old.tar.bz2", "$fixtures\archives\new.tar.bz2", '--no-color') 'tar.bz2 archive:'
Invoke-Case 'tar.xz archive' 1 @("$fixtures\archives\old.tar.xz", "$fixtures\archives\new.tar.xz", '--no-color') 'tar.xz archive:'
Invoke-Case '7z archive' 1 @("$fixtures\archives\old.7z", "$fixtures\archives\new.7z", '--no-color') '7z archive:'
Invoke-Case 'deb package' 1 @("$fixtures\deb\old.deb", "$fixtures\deb\new.deb", '--no-color') 'deb archive:'
Invoke-Case 'tar.zst archive' 1 @("$fixtures\archive-zst\old.tar.zst", "$fixtures\archive-zst\new.tar.zst", '--no-color') 'tar.zst archive:'
Invoke-Case 'tar.zst cross-compression equal' 0 @("$fixtures\archive-zst\old.tar.zst", "$fixtures\archive-zst\old.tar.gz", '--quiet')
Invoke-Case 'zstd single file' 1 @("$fixtures\archive-zst\old.log.zst", "$fixtures\archive-zst\new.log.zst", '--no-color') 'text:'
Invoke-Case 'iso image' 1 @("$fixtures\iso\old.iso", "$fixtures\iso\new.iso", '--no-color') 'iso archive:'
Invoke-Case 'iso equal' 0 @("$fixtures\iso\old.iso", "$fixtures\iso\old.iso", '--quiet')
Invoke-Case 'bzip2 single file' 1 @("$fixtures\compression\old.txt.bz2", "$fixtures\compression\new.txt.bz2", '--no-color') 'text:'
Invoke-Case 'xz single file' 1 @("$fixtures\compression\old.txt.xz", "$fixtures\compression\new.txt.xz", '--no-color') 'text:'
Invoke-Case 'cpio archive' 1 @("$fixtures\cpio\old.cpio", "$fixtures\cpio\new.cpio", '--no-color') 'cpio archive:'
Invoke-Case 'cpio.gz archive' 1 @("$fixtures\cpio\old.cpio.gz", "$fixtures\cpio\new.cpio.gz", '--no-color') 'cpio archive:'
Invoke-Case 'tar.lz4 archive' 1 @("$fixtures\lz4\old.tar.lz4", "$fixtures\lz4\new.tar.lz4", '--no-color') 'tar.lz4 archive:'
Invoke-Case 'lz4 single file' 1 @("$fixtures\lz4\old.log.lz4", "$fixtures\lz4\new.log.lz4", '--no-color') 'text:'
Invoke-Case 'notebook cells' 1 @("$fixtures\notebook\old.ipynb", "$fixtures\notebook\new.ipynb", '--no-color') 'notebook:'
Invoke-Case 'token mode' 1 @("$fixtures\code\old.rs", "$fixtures\code\new.rs", '--mode', 'token', '--ignore-comments', '--no-color') 'rust tokens: +1 -1'
Invoke-Case 'tree mode' 1 @("$fixtures\code\old.rs", "$fixtures\code\new.rs", '--mode', 'tree', '--ignore-comments', '--no-color') 'rust tree: +0 -0 ~1'
Invoke-Case 'graph mode' 1 @("$fixtures\code\old.rs", "$fixtures\code\new.rs", '--mode', 'graph', '--ignore-comments', '--no-color') '1 -> 2'
Invoke-Case 'ignore comments auto' 1 @("$fixtures\code\old.rs", "$fixtures\code\new.rs", '--ignore-comments', '--no-color') 'rust tokens: +1 -1'
Invoke-Case 'unified output' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--format', 'unified') '--- '
Invoke-Case 'tui fallback' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--format', 'tui', '--no-color') '@@ '
Invoke-Case 'markdown output' 1 @("$fixtures\json\old.json", "$fixtures\json\new.json", '--format', 'markdown') '# DiffEverything report'
Invoke-Case 'html output' 1 @("$fixtures\dir\left", "$fixtures\dir\right", '--format', 'html') '</html>'
Invoke-Case 'name-only' 1 @("$fixtures\dir\left", "$fixtures\dir\right", '--name-only') 'README.md'
Invoke-Case 'exclude glob' 1 @("$fixtures\dir\left", "$fixtures\dir\right", '--exclude', '*.txt', '--no-color') 'README.md'
Invoke-Case 'summary-only' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--summary-only') 'changed paths:'

$quietOutput = & $Exe "$fixtures\text\old.txt" "$fixtures\text\new.txt" --quiet 2>&1 | Out-String
if ($LASTEXITCODE -eq 1 -and $quietOutput.Trim().Length -eq 0) {
    $script:passes++
    Write-Host '[PASS] quiet mode'
} else {
    $script:failures++
    Write-Host "[FAIL] quiet mode (exit=$LASTEXITCODE output=$quietOutput)"
}

$temp = Join-Path ([System.IO.Path]::GetTempPath()) 'diff-everything-smoke'
New-Item -ItemType Directory -Force $temp | Out-Null
$nbOld = Join-Path $temp 'moved-old.ipynb'
$nbNew = Join-Path $temp 'moved-new.ipynb'
Set-Content -Path $nbOld -Value '{"cells":[{"cell_type":"markdown","id":"a","metadata":{},"source":["a"]},{"cell_type":"markdown","id":"b","metadata":{},"source":["b"]}],"metadata":{},"nbformat":4,"nbformat_minor":5}' -NoNewline
Set-Content -Path $nbNew -Value '{"cells":[{"cell_type":"markdown","id":"b","metadata":{},"source":["b"]},{"cell_type":"markdown","id":"a","metadata":{},"source":["a"]}],"metadata":{},"nbformat":4,"nbformat_minor":5}' -NoNewline
Invoke-Case 'notebook moved cells' 1 @($nbOld, $nbNew, '--no-color') 'moved 1'
Invoke-Case 'heatmap output' 1 @("$fixtures\images\old.png", "$fixtures\images\new.png", '--image-heatmap', "$temp\heatmap.png", '--image-side-by-side', "$temp\side-by-side.png", '--quiet')
if (Test-Path "$temp\heatmap.png") { $script:passes++; Write-Host '[PASS] heatmap file exists' } else { $script:failures++; Write-Host '[FAIL] heatmap file exists' }
if (Test-Path "$temp\side-by-side.png") { $script:passes++; Write-Host '[PASS] side-by-side file exists' } else { $script:failures++; Write-Host '[FAIL] side-by-side file exists' }
Invoke-Case 'image threshold' 0 @("$fixtures\images\old.png", "$fixtures\images\new.png", '--image-threshold', '1.0', '--no-color') 'EQUAL'
Invoke-Case 'image regions' 1 @("$fixtures\images\old.png", "$fixtures\images\new.png", '--format', 'json') '"regions": ['

Set-Content -Path "$temp\ignore-left.txt" -Value "Hello`n    value = 1`n" -NoNewline
Set-Content -Path "$temp\ignore-right.txt" -Value "hello`n`tvalue = 1`n" -NoNewline
Invoke-Case 'ignore whitespace/case' 0 @("$temp\ignore-left.txt", "$temp\ignore-right.txt", '--ignore-whitespace', '--ignore-case', '--quiet')

Set-Content -Path "$temp\aligned-old.json" -Value '[{"id":1,"name":"alpha"},{"id":2,"name":"beta"},{"id":3,"name":"gamma"}]' -NoNewline
Set-Content -Path "$temp\aligned-new.json" -Value '[{"id":0,"name":"zero"},{"id":1,"name":"alpha"},{"id":2,"name":"beta"},{"id":3,"name":"gamma"}]' -NoNewline
Invoke-Case 'json array alignment' 1 @("$temp\aligned-old.json", "$temp\aligned-new.json", '--no-color') '1 structured changes'

Invoke-Case 'merge clean' 0 @("$fixtures\merge\left.txt", "$fixtures\merge\right.txt", '--base', "$fixtures\merge\base.txt") 'right three'
Invoke-Case 'merge conflict' 1 @("$fixtures\merge\conflict_left.txt", "$fixtures\merge\conflict_right.txt", '--base', "$fixtures\merge\base.txt") '<<<<<<< ours'
Invoke-Case 'bsdiff patch' 0 @("$fixtures\binary\old.bin", "$fixtures\binary\new.bin", '--patch-output', "$temp\change.patch")
if (Test-Path "$temp\change.patch") { $script:passes++; Write-Host '[PASS] patch file exists' } else { $script:failures++; Write-Host '[FAIL] patch file exists' }

# Folder sync plan + apply
$syncLeft = Join-Path $temp 'sync-left'
$syncRight = Join-Path $temp 'sync-right'
$syncBackup = Join-Path $temp 'sync-backup'
Remove-Item $syncLeft, $syncRight, $syncBackup -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $syncLeft, $syncRight | Out-Null
Copy-Item "$fixtures\dir\left\*" $syncLeft -Recurse -Force
Copy-Item "$fixtures\dir\right\*" $syncRight -Recurse -Force
Invoke-Case 'sync plan' 1 @($syncLeft, $syncRight, '--sync', 'mirror-left-to-right') 'DELETE new-only.txt'
Invoke-Case 'sync apply' 0 @($syncLeft, $syncRight, '--sync', 'mirror-left-to-right', '--sync-apply', '--backup-dir', $syncBackup) 'sync applied'
if ((Test-Path "$syncRight\old-only.txt") -and -not (Test-Path "$syncRight\new-only.txt")) {
    $script:passes++; Write-Host '[PASS] sync result'
} else { $script:failures++; Write-Host '[FAIL] sync result' }

# Folder three-way merge
$mergeBase = Join-Path $temp 'merge-base'
$mergeOurs = Join-Path $temp 'merge-ours'
$mergeTheirs = Join-Path $temp 'merge-theirs'
$mergeOut = Join-Path $temp 'merge-out'
Remove-Item $mergeBase, $mergeOurs, $mergeTheirs, $mergeOut -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $mergeBase, $mergeOurs, $mergeTheirs | Out-Null
foreach ($root in @($mergeBase, $mergeOurs, $mergeTheirs)) {
    Set-Content -Path (Join-Path $root 'clean.txt') -Value 'base' -NoNewline
    Set-Content -Path (Join-Path $root 'both.txt') -Value 'base' -NoNewline
}
Set-Content -Path (Join-Path $mergeOurs 'clean.txt') -Value 'base' -NoNewline
Set-Content -Path (Join-Path $mergeTheirs 'clean.txt') -Value 'theirs' -NoNewline
Set-Content -Path (Join-Path $mergeOurs 'both.txt') -Value 'ours' -NoNewline
Set-Content -Path (Join-Path $mergeTheirs 'both.txt') -Value 'theirs' -NoNewline
Invoke-Case 'folder merge conflict' 1 @($mergeOurs, $mergeTheirs, '--base', $mergeBase, '--merge-output', $mergeOut, '--no-color') 'conflict: both.txt'
$mergeConflict = Get-Content (Join-Path $mergeOut 'both.txt') -Raw -ErrorAction SilentlyContinue
if ($mergeConflict -and $mergeConflict.Contains('<<<<<<< ours') -and (Get-Content (Join-Path $mergeOut 'clean.txt') -Raw) -eq 'theirs') {
    $script:passes++; Write-Host '[PASS] folder merge output'
} else { $script:failures++; Write-Host '[FAIL] folder merge output' }

$cleanBase = Join-Path $temp 'merge-clean-base'
$cleanOurs = Join-Path $temp 'merge-clean-ours'
$cleanTheirs = Join-Path $temp 'merge-clean-theirs'
$cleanOut = Join-Path $temp 'merge-clean-out'
Remove-Item $cleanBase, $cleanOurs, $cleanTheirs, $cleanOut -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $cleanBase, $cleanOurs, $cleanTheirs | Out-Null
Set-Content -Path (Join-Path $cleanBase 'a.txt') -Value 'base' -NoNewline
Set-Content -Path (Join-Path $cleanOurs 'a.txt') -Value 'ours' -NoNewline
Set-Content -Path (Join-Path $cleanTheirs 'a.txt') -Value 'base' -NoNewline
Invoke-Case 'folder merge clean' 0 @($cleanOurs, $cleanTheirs, '--base', $cleanBase, '--merge-output', $cleanOut, '--no-color') 'folder merge:'

# Folder-level bsdiff patch create/apply
$patchLeft = Join-Path $temp 'patch-left'
$patchRight = Join-Path $temp 'patch-right'
$patchFile = Join-Path $temp 'folder.patch'
$patchOut = Join-Path $temp 'patch-applied'
Remove-Item $patchLeft, $patchRight, $patchFile, $patchOut -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $patchLeft, $patchRight | Out-Null
Set-Content -Path (Join-Path $patchLeft 'same.txt') -Value 'same' -NoNewline
Set-Content -Path (Join-Path $patchRight 'same.txt') -Value 'same' -NoNewline
Set-Content -Path (Join-Path $patchLeft 'change.txt') -Value 'old' -NoNewline
Set-Content -Path (Join-Path $patchRight 'change.txt') -Value 'new' -NoNewline
Set-Content -Path (Join-Path $patchLeft 'removed.txt') -Value 'left-only' -NoNewline
Set-Content -Path (Join-Path $patchRight 'added.txt') -Value 'right-only' -NoNewline
Invoke-Case 'folder patch create' 0 @($patchLeft, $patchRight, '--patch-output', $patchFile, '--no-color') 'folder patch created'
Invoke-Case 'folder patch apply' 0 @($patchLeft, '--apply-patch', $patchFile, '--merge-output', $patchOut, '--no-color') 'folder patch applied'
if ((Get-Content (Join-Path $patchOut 'change.txt') -Raw) -eq 'new' -and (Test-Path (Join-Path $patchOut 'added.txt')) -and -not (Test-Path (Join-Path $patchOut 'removed.txt')) -and (Test-Path (Join-Path $patchLeft 'removed.txt'))) {
    $script:passes++; Write-Host '[PASS] folder patch output'
} else { $script:failures++; Write-Host '[FAIL] folder patch output' }

# Pure-Rust HDIFF13 encoder + apply (hpatchz-compatible)
$hdiffRustPatch = Join-Path $temp 'hdiff-rust.hdiff'
$hdiffRustOut = Join-Path $temp 'hdiff-rust-out.bin'
Remove-Item $hdiffRustPatch, $hdiffRustOut -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff rust create' 0 @("$fixtures\binary\old.bin", "$fixtures\binary\new.bin", '--patch-format', 'hdiff-rust', '--patch-output', $hdiffRustPatch, '--no-color') 'hdiff-rust patch created'
Invoke-Case 'hdiff rust apply' 0 @("$fixtures\binary\old.bin", '--apply-patch', $hdiffRustPatch, '--merge-output', $hdiffRustOut, '--no-color') 'hdiff patch applied (pure Rust)'
if ((Get-FileHash $hdiffRustOut).Hash -eq (Get-FileHash "$fixtures\binary\new.bin").Hash) {
    $script:passes++; Write-Host '[PASS] hdiff rust output'
} else { $script:failures++; Write-Host '[FAIL] hdiff rust output' }

$hdiffRustDirOld = Join-Path $temp 'hdiff-rust-dir-old'
$hdiffRustDirNew = Join-Path $temp 'hdiff-rust-dir-new'
$hdiffRustDirOut = Join-Path $temp 'hdiff-rust-dir-out'
Remove-Item $hdiffRustDirOld, $hdiffRustDirNew, $hdiffRustDirOut -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$hdiffRustDirOld\nested", "$hdiffRustDirNew\sub" | Out-Null
Set-Content -Path "$hdiffRustDirOld\keep.txt" -Value 'old' -NoNewline
Set-Content -Path "$hdiffRustDirOld\same.txt" -Value 'same' -NoNewline
Set-Content -Path "$hdiffRustDirOld\remove.txt" -Value 'remove' -NoNewline
Set-Content -Path "$hdiffRustDirNew\keep.txt" -Value 'new' -NoNewline
Set-Content -Path "$hdiffRustDirNew\same.txt" -Value 'same' -NoNewline
Set-Content -Path "$hdiffRustDirNew\added.txt" -Value 'added' -NoNewline
Set-Content -Path "$hdiffRustDirNew\sub\deep.txt" -Value 'deep' -NoNewline
$hdiffRustDirPatch = Join-Path $temp 'hdiff-rust-dir.hdiff'
Remove-Item $hdiffRustDirPatch -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff rust dir create' 0 @($hdiffRustDirOld, $hdiffRustDirNew, '--patch-format', 'hdiff-rust', '--patch-output', $hdiffRustDirPatch, '--no-color') 'hdiff-rust directory patch created'
Invoke-Case 'hdiff rust dir apply' 0 @($hdiffRustDirOld, '--apply-patch', $hdiffRustDirPatch, '--merge-output', $hdiffRustDirOut, '--no-color') 'pure Rust'
if ((Test-Path "$hdiffRustDirOut\sub\deep.txt") -and (Test-Path "$hdiffRustDirOut\added.txt") -and (Test-Path "$hdiffRustDirOut\same.txt") -and -not (Test-Path "$hdiffRustDirOut\remove.txt")) {
    $script:passes++; Write-Host '[PASS] hdiff rust dir output'
} else { $script:failures++; Write-Host '[FAIL] hdiff rust dir output' }

$hdiffRustDirZstdPatch = Join-Path $temp 'hdiff-rust-dir-zstd.hdiff'
$hdiffRustDirZstdOut = Join-Path $temp 'hdiff-rust-dir-zstd-out'
Remove-Item $hdiffRustDirZstdPatch, $hdiffRustDirZstdOut -Recurse -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff rust dir zstd create' 0 @($hdiffRustDirOld, $hdiffRustDirNew, '--patch-format', 'hdiff-rust', '--patch-compress', 'zstd', '--patch-output', $hdiffRustDirZstdPatch, '--no-color') 'directory patch created'
Invoke-Case 'hdiff rust dir zstd apply' 0 @($hdiffRustDirOld, '--apply-patch', $hdiffRustDirZstdPatch, '--merge-output', $hdiffRustDirZstdOut, '--no-color') 'pure Rust'
if ((Test-Path "$hdiffRustDirZstdOut\sub\deep.txt") -and (Test-Path "$hdiffRustDirZstdOut\same.txt") -and -not (Test-Path "$hdiffRustDirZstdOut\remove.txt")) {
    $script:passes++; Write-Host '[PASS] hdiff rust dir zstd output'
} else { $script:failures++; Write-Host '[FAIL] hdiff rust dir zstd output' }

$hdiffRustDirLzma2Patch = Join-Path $temp 'hdiff-rust-dir-lzma2.hdiff'
$hdiffRustDirLzma2Out = Join-Path $temp 'hdiff-rust-dir-lzma2-out'
Remove-Item $hdiffRustDirLzma2Patch, $hdiffRustDirLzma2Out -Recurse -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff rust dir lzma2 create' 0 @($hdiffRustDirOld, $hdiffRustDirNew, '--patch-format', 'hdiff-rust', '--patch-compress', 'lzma2', '--patch-output', $hdiffRustDirLzma2Patch, '--no-color') 'directory patch created'
Invoke-Case 'hdiff rust dir lzma2 apply' 0 @($hdiffRustDirOld, '--apply-patch', $hdiffRustDirLzma2Patch, '--merge-output', $hdiffRustDirLzma2Out, '--no-color') 'pure Rust'
if ((Test-Path "$hdiffRustDirLzma2Out\sub\deep.txt") -and (Test-Path "$hdiffRustDirLzma2Out\same.txt") -and -not (Test-Path "$hdiffRustDirLzma2Out\remove.txt")) {
    $script:passes++; Write-Host '[PASS] hdiff rust dir lzma2 output'
} else { $script:failures++; Write-Host '[FAIL] hdiff rust dir lzma2 output' }

$hdiffRustDirBzip2Patch = Join-Path $temp 'hdiff-rust-dir-bzip2.hdiff'
$hdiffRustDirBzip2Out = Join-Path $temp 'hdiff-rust-dir-bzip2-out'
Remove-Item $hdiffRustDirBzip2Patch, $hdiffRustDirBzip2Out -Recurse -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff rust dir bzip2 create' 0 @($hdiffRustDirOld, $hdiffRustDirNew, '--patch-format', 'hdiff-rust', '--patch-compress', 'bzip2', '--patch-output', $hdiffRustDirBzip2Patch, '--no-color') 'directory patch created'
Invoke-Case 'hdiff rust dir bzip2 apply' 0 @($hdiffRustDirOld, '--apply-patch', $hdiffRustDirBzip2Patch, '--merge-output', $hdiffRustDirBzip2Out, '--no-color') 'pure Rust'
if ((Test-Path "$hdiffRustDirBzip2Out\sub\deep.txt") -and (Test-Path "$hdiffRustDirBzip2Out\same.txt") -and -not (Test-Path "$hdiffRustDirBzip2Out\remove.txt")) {
    $script:passes++; Write-Host '[PASS] hdiff rust dir bzip2 output'
} else { $script:failures++; Write-Host '[FAIL] hdiff rust dir bzip2 output' }

# Pure-Rust uncompressed HDIFF13 apply (no external tool needed)
$hdiffPureOut = Join-Path $temp 'hdiff-pure-out.bin'
Remove-Item $hdiffPureOut -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff pure rust apply' 0 @("$fixtures\binary\old.bin", '--apply-patch', "$fixtures\hdiff\change.hdiff", '--merge-output', $hdiffPureOut, '--no-color') 'pure Rust'
if ((Get-FileHash $hdiffPureOut).Hash -eq (Get-FileHash "$fixtures\binary\new.bin").Hash) {
    $script:passes++; Write-Host '[PASS] hdiff pure rust output'
} else { $script:failures++; Write-Host '[FAIL] hdiff pure rust output' }

$hdiffZstdOut = Join-Path $temp 'hdiff-zstd-out.bin'
Remove-Item $hdiffZstdOut -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff pure rust zstd apply' 0 @("$fixtures\hdiff\large-old.bin", '--apply-patch', "$fixtures\hdiff\large-zstd.hdiff", '--merge-output', $hdiffZstdOut, '--no-color') 'pure Rust'
if ((Get-FileHash $hdiffZstdOut).Hash -eq (Get-FileHash "$fixtures\hdiff\large-new.bin").Hash) {
    $script:passes++; Write-Host '[PASS] hdiff pure rust zstd output'
} else { $script:failures++; Write-Host '[FAIL] hdiff pure rust zstd output' }

$hdiffLzma2Out = Join-Path $temp 'hdiff-lzma2-out.bin'
Remove-Item $hdiffLzma2Out -Force -ErrorAction SilentlyContinue
Invoke-Case 'hdiff pure rust lzma2 apply' 0 @("$fixtures\hdiff\large-old.bin", '--apply-patch', "$fixtures\hdiff\large-lzma2.hdiff", '--merge-output', $hdiffLzma2Out, '--no-color') 'pure Rust'
if ((Get-FileHash $hdiffLzma2Out).Hash -eq (Get-FileHash "$fixtures\hdiff\large-new.bin").Hash) {
    $script:passes++; Write-Host '[PASS] hdiff pure rust lzma2 output'
} else { $script:failures++; Write-Host '[FAIL] hdiff pure rust lzma2 output' }

# HDiffPatch-compatible single-file patch (optional external tools)
$hdiffRoot = Join-Path $PSScriptRoot '..\testdata\external\hdiffpatch'
$hdiffz = Get-ChildItem $hdiffRoot -Recurse -Filter hdiffz.exe -ErrorAction SilentlyContinue | Select-Object -First 1
$hpatchz = Get-ChildItem $hdiffRoot -Recurse -Filter hpatchz.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if ($hdiffz -and $hpatchz) {
    $hdiffPatch = Join-Path $temp 'hdiff.bin'
    $hdiffOut = Join-Path $temp 'hdiff-out.bin'
    Remove-Item $hdiffPatch, $hdiffOut -Force -ErrorAction SilentlyContinue
    Invoke-Case 'hdiff patch create' 0 @("$fixtures\binary\old.bin", "$fixtures\binary\new.bin", '--patch-format', 'hdiff', '--hdiffz', $hdiffz.FullName, '--patch-output', $hdiffPatch, '--no-color') 'hdiff patch created'
    Invoke-Case 'hdiff patch apply' 0 @("$fixtures\binary\old.bin", '--patch-format', 'hdiff', '--hpatchz', $hpatchz.FullName, '--apply-patch', $hdiffPatch, '--merge-output', $hdiffOut, '--no-color') 'hdiff patch applied'
    if ((Get-FileHash $hdiffOut).Hash -eq (Get-FileHash "$fixtures\binary\new.bin").Hash) {
        $script:passes++; Write-Host '[PASS] hdiff patch output'
    } else { $script:failures++; Write-Host '[FAIL] hdiff patch output' }

    $officialDirPatch = Join-Path $temp 'official-dir.hdiff'
    $officialDirOut = Join-Path $temp 'official-dir-out'
    Remove-Item $officialDirPatch, $officialDirOut -Recurse -Force -ErrorAction SilentlyContinue
    Invoke-Case 'hdiff official dir create' 0 @($hdiffRustDirOld, $hdiffRustDirNew, '--patch-format', 'hdiff', '--hdiffz', $hdiffz.FullName, '--patch-output', $officialDirPatch, '--no-color') 'hdiff patch created'
    Invoke-Case 'hdiff official dir apply pure rust' 0 @($hdiffRustDirOld, '--apply-patch', $officialDirPatch, '--merge-output', $officialDirOut, '--no-color') 'pure Rust'
    if ((Test-Path "$officialDirOut\keep.txt") -and (Test-Path "$officialDirOut\same.txt") -and (Test-Path "$officialDirOut\sub\deep.txt") -and -not (Test-Path "$officialDirOut\remove.txt")) {
        $script:passes++; Write-Host '[PASS] hdiff official dir output'
    } else { $script:failures++; Write-Host '[FAIL] hdiff official dir output' }
} else {
    Write-Host '[SKIP] HDiffPatch tools not downloaded'
}

# Session save/load
$sessionPath = Join-Path $temp 'session.json'
Invoke-Case 'session save' 1 @("$fixtures\json\old.json", "$fixtures\json\new.json", '--save-session', $sessionPath, '--format', 'json')
Invoke-Case 'session load' 1 @('--load-session', $sessionPath, '--format', 'json') '"status": "different"'

# Optional OCR / PDF visual cases (tools downloaded by fetch-testdata.ps1)
$tesseract = Get-ChildItem (Join-Path $PSScriptRoot '..\testdata\external\tesseract') -Recurse -Filter tesseract.exe -ErrorAction SilentlyContinue | Select-Object -First 1
$tessdata = Join-Path $PSScriptRoot '..\testdata\external\tesseract\tessdata'
$pdftoppm = Get-ChildItem (Join-Path $PSScriptRoot '..\testdata\external\poppler') -Recurse -Filter pdftoppm.exe -ErrorAction SilentlyContinue | Select-Object -First 1
if ($tesseract) {
    Invoke-Case 'ocr images' 1 @("$fixtures\ocr\old.png", "$fixtures\ocr\new.png", '--ocr', '--tesseract', $tesseract.FullName, '--tessdata', $tessdata, '--no-color') 'Hello world'
}
if ($tesseract -and $pdftoppm) {
    Invoke-Case 'pdf ocr' 1 @("$fixtures\pdf\old.pdf", "$fixtures\pdf\new.pdf", '--ocr', '--tesseract', $tesseract.FullName, '--tessdata', $tessdata, '--pdftoppm', $pdftoppm.FullName, '--pdf-max-pages', '1', '--no-color') 'pdf-ocr:'
}
if ($pdftoppm) {
    $pdfVisual = Join-Path $temp 'pdf-visual'
    Invoke-Case 'pdf visual' 1 @("$fixtures\pdf\old.pdf", "$fixtures\pdf\new.pdf", '--pdf-visual', '--pdftoppm', $pdftoppm.FullName, '--pdf-output-dir', $pdfVisual, '--pdf-dpi', '40', '--no-color') 'PDF visual diff'
    $pdfHtml = Get-Content (Join-Path $pdfVisual 'report.html') -Raw -ErrorAction SilentlyContinue
    if ((Test-Path (Join-Path $pdfVisual 'report.html')) -and (Test-Path (Join-Path $pdfVisual 'report.pdf')) -and (Test-Path (Join-Path $pdfVisual 'report-overlay.pdf')) -and (Test-Path (Join-Path $pdfVisual 'report-vector.pdf')) -and $pdfHtml.Contains('data:image/png;base64,')) {
        $script:passes++; Write-Host '[PASS] pdf visual report'
    } else { $script:failures++; Write-Host '[FAIL] pdf visual report' }
}

# Remote inputs against local servers (skipped when Python or optional packages are missing)
function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    return $port
}

function Wait-ForPort {
    param([int]$Port, [int]$TimeoutMs = 10000)
    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    while ((Get-Date) -lt $deadline) {
        $client = [System.Net.Sockets.TcpClient]::new()
        try {
            $task = $client.ConnectAsync('127.0.0.1', $Port)
            if ($task.Wait(250) -and $client.Connected) { return $true }
        } catch {
        } finally {
            $client.Dispose()
        }
        Start-Sleep -Milliseconds 100
    }
    return $false
}

function Quote-Arg {
    param([string]$Value)
    if ($Value -match '\s') { return '"' + $Value + '"' }
    return $Value
}

function Start-TestServer {
    param([string]$Script, [string]$Root, [int]$Port, [string[]]$Extra = @())
    $arguments = @(
        (Quote-Arg (Join-Path $PSScriptRoot $Script)),
        '--root', (Quote-Arg $Root),
        '--port', "$Port"
    ) + $Extra
    $stdout = Join-Path $temp "server-$Port.out"
    $stderr = Join-Path $temp "server-$Port.err"
    return Start-Process -FilePath $python.Source -ArgumentList $arguments -PassThru `
        -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
}

$python = Get-Command python -ErrorAction SilentlyContinue
if ($python) {
    Invoke-Case 'external comparator' 1 @("$fixtures\text\old.txt", "$fixtures\text\new.txt", '--external-tool', $python.Source, '--external-arg', '-c', '--external-arg', 'import sys; print("external-diff"); sys.exit(1)') 'external-diff'
    $webdavPort = Get-FreePort
    $webdav = Start-TestServer 'webdav-test-server.py' (Join-Path $fixtures 'dir') $webdavPort
    if (Wait-ForPort $webdavPort) {
        Invoke-Case 'webdav file' 1 @("webdav://127.0.0.1:$webdavPort/left/README.md", "webdav://127.0.0.1:$webdavPort/right/README.md", '--no-color') '@@ '
        Invoke-Case 'webdav directory' 1 @("webdav://127.0.0.1:$webdavPort/left/", "webdav://127.0.0.1:$webdavPort/right/", '--no-color') 'old-only.txt'
    } else {
        Write-Host '[SKIP] webdav server did not start'
    }
    Stop-Process -Id $webdav.Id -Force -ErrorAction SilentlyContinue

    & $python.Source -c 'import asyncssh' 2>$null
    if ($LASTEXITCODE -eq 0) {
        $sftpPort = Get-FreePort
        $sftpHostKey = Join-Path $temp 'sftp-host-key.txt'
        $sftp = Start-TestServer 'sftp-test-server.py' (Join-Path $fixtures 'dir') $sftpPort @('--user', 'test', '--password', 'test', '--host-key-out', $sftpHostKey)
        if ((Wait-ForPort $sftpPort) -and (Test-Path $sftpHostKey)) {
            Invoke-Case 'sftp file' 1 @("sftp://test:test@127.0.0.1:$sftpPort/left/README.md", "sftp://test:test@127.0.0.1:$sftpPort/right/README.md", '--no-color') '@@ '
            Invoke-Case 'sftp directory' 1 @("sftp://test:test@127.0.0.1:$sftpPort/left/", "sftp://test:test@127.0.0.1:$sftpPort/right/", '--no-color') 'old-only.txt'
            $knownHosts = Join-Path $temp 'known_hosts'
            $publicKey = (Get-Content $sftpHostKey -Raw).Trim()
            Set-Content -Path $knownHosts -Value "[127.0.0.1]:$sftpPort $publicKey"
            Invoke-Case 'sftp known_hosts' 1 @("sftp://test:test@127.0.0.1:$sftpPort/left/README.md", "sftp://test:test@127.0.0.1:$sftpPort/right/README.md", '--sftp-known-hosts', $knownHosts, '--no-color') '@@ '
        } else {
            Write-Host '[SKIP] sftp server did not start'
        }
        Stop-Process -Id $sftp.Id -Force -ErrorAction SilentlyContinue
    } else {
        Write-Host '[SKIP] python asyncssh not installed'
    }

    & $python.Source -c 'import impacket' 2>$null
    if ($LASTEXITCODE -eq 0) {
        $smbPort = Get-FreePort
        $smb = Start-TestServer 'smb-test-server.py' (Join-Path $fixtures 'dir') $smbPort @('--user', 'test', '--password', 'test')
        if (Wait-ForPort $smbPort) {
            Invoke-Case 'smb file' 1 @("smb://test:test@127.0.0.1:$smbPort/data/left/src/main.rs", "smb://test:test@127.0.0.1:$smbPort/data/right/src/main.rs", '--no-color') '@@ '
            Invoke-Case 'smb directory' 1 @("smb://test:test@127.0.0.1:$smbPort/data/left", "smb://test:test@127.0.0.1:$smbPort/data/right", '--no-color') 'old-only.txt'
        } else {
            Write-Host '[SKIP] smb server did not start'
        }
        Stop-Process -Id $smb.Id -Force -ErrorAction SilentlyContinue
    } else {
        Write-Host '[SKIP] python impacket not installed'
    }
} else {
    Write-Host '[SKIP] python not found'
}

# Real files downloaded by scripts/fetch-testdata.ps1
if (Test-Path $external) {
    Invoke-Case 'external pdf equal' 0 @("$external\dummy.pdf", "$external\dummy.pdf", '--quiet')
    Invoke-Case 'external pdf diff' 1 @("$external\dummy.pdf", "$external\tracemonkey.pdf", '--quiet')
    Invoke-Case 'external docx equal' 0 @("$external\python-docx-test.docx", "$external\python-docx-test.docx", '--quiet')
    Invoke-Case 'external docx diff' 1 @("$external\python-docx-test.docx", "$external\python-docx-images.docx", '--quiet')
    Invoke-Case 'external xlsx equal' 0 @("$external\calamine-any-sheets.xlsx", "$external\calamine-any-sheets.xlsx", '--quiet')
    Invoke-Case 'external xlsx diff' 1 @("$external\calamine-any-sheets.xlsx", "$external\calamine-header-row.xlsx", '--quiet')
    Invoke-Case 'external xls/ods diff' 1 @("$external\calamine-any-sheets.xls", "$external\calamine-any-sheets.ods", '--quiet')
    Invoke-Case 'external csv equal' 0 @("$external\iris.csv", "$external\iris.csv", '--quiet')
    Invoke-Case 'external yaml equal' 0 @("$external\nginx-app.yaml", "$external\nginx-app.yaml", '--quiet')
    Invoke-Case 'external xml equal' 0 @("$external\note.xml", "$external\note.xml", '--quiet')
    Invoke-Case 'external json equal' 0 @("$external\todo.json", "$external\todo.json", '--quiet')
    Invoke-Case 'external images diff' 1 @("$external\image-a.jpg", "$external\image-b.jpg", '--quiet')
    Invoke-Case 'external tar.gz equal' 0 @("$external\hello-2.12.tar.gz", "$external\hello-2.12.tar.gz", '--quiet')
    Invoke-Case 'external deb equal' 0 @("$external\hello_2.10-2_amd64.deb", "$external\hello_2.10-2_amd64.deb", '--quiet')
    Invoke-Case 'external deb diff' 1 @("$external\hello_2.10-2_amd64.deb", "$external\hello_2.10-3_amd64.deb", '--quiet')
    Invoke-Case 'external tar.zst equal' 0 @("$external\which-2.22-1-x86_64.pkg.tar.zst", "$external\which-2.22-1-x86_64.pkg.tar.zst", '--quiet')
    Invoke-Case 'external tar.zst diff' 1 @("$external\which-2.22-1-x86_64.pkg.tar.zst", "$external\which-2.25-1-x86_64.pkg.tar.zst", '--quiet')
    Invoke-Case 'external rpm equal' 0 @("$external\hello-2.12.1-2.fc39.x86_64.rpm", "$external\hello-2.12.1-2.fc39.x86_64.rpm", '--quiet')
    Invoke-Case 'external rpm diff' 1 @("$external\hello-2.12.1-2.fc39.x86_64.rpm", "$external\hello-2.12.1-4.fc40.x86_64.rpm", '--quiet')
    Invoke-Case 'external rpm xz equal' 0 @("$external\which-2.21-12.el8.x86_64.rpm", "$external\which-2.21-12.el8.x86_64.rpm", '--quiet')
    Invoke-Case 'external rpm xz diff' 1 @("$external\which-2.21-12.el8.x86_64.rpm", "$external\which-2.21-16.el8.x86_64.rpm", '--quiet')
    Invoke-Case 'external zip equal' 0 @("$external\open-diff-master.zip", "$external\open-diff-master.zip", '--quiet')
} else {
    Write-Host "[SKIP] external testdata not found: $external"
}

Invoke-Case 'decompile full -y' 1 @("$fixtures\executable\hello-old", "$fixtures\executable\hello-new", '--detail', 'full', '-y', '--no-color') 'decompiled code (embedded)'
Invoke-Case 'apk native decompile' 1 @("$fixtures\apk\native-old.apk", "$fixtures\apk\native-new.apk", '--detail', 'full', '-y', '--no-color') 'lib/x86_64/libhello.so'

Write-Host "`n$($script:passes) passed, $($script:failures) failed"
if ($script:failures -gt 0) { exit 1 }
