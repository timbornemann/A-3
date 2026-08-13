param(
    [string]$ExecutablePath = '',
    [ValidateRange(1, 300)]
    [int]$WarmupSeconds = 15,
    [ValidateRange(1, 300)]
    [int]$SampleCount = 30,
    [ValidateRange(1, 60)]
    [int]$SampleIntervalSeconds = 1
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
    $ExecutablePath = [System.IO.Path]::GetFullPath(
        (Join-Path $PSScriptRoot '..\..\..\target\release\a3-desktop.exe')
    )
}
$resolvedExecutable = (Resolve-Path -LiteralPath $ExecutablePath).Path

$modelProcessPattern = '^(ollama|lmstudio|llama-server|koboldcpp|local-ai)$'
$modelProcesses = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -match $modelProcessPattern
    })
$observedModelServers = @($modelProcesses.ProcessName | Sort-Object -Unique)

function Get-ProcessTreeIds {
    param(
        [int]$RootProcessId,
        [object[]]$ProcessTable
    )

    $ids = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ids.Add($RootProcessId)
    $expanded = $true
    while ($expanded) {
        $expanded = $false
        foreach ($row in $ProcessTable) {
            if ($ids.Contains([int]$row.ParentProcessId) -and $ids.Add([int]$row.ProcessId)) {
                $expanded = $true
            }
        }
    }
    return @($ids)
}

function Get-Median {
    param([double[]]$Values)

    $sorted = @($Values | Sort-Object)
    $middle = [int][math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) {
        return $sorted[$middle]
    }
    return ($sorted[$middle - 1] + $sorted[$middle]) / 2
}

$startedAtUtc = [DateTimeOffset]::UtcNow
$app = Start-Process -FilePath $resolvedExecutable -PassThru -WindowStyle Hidden
try {
    Start-Sleep -Seconds $WarmupSeconds
    $samples = for ($sample = 1; $sample -le $SampleCount; $sample += 1) {
        $processTable = @(Get-CimInstance Win32_Process)
        $processIds = Get-ProcessTreeIds -RootProcessId $app.Id -ProcessTable $processTable
        $performanceRows = @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process)
        $processRows = @($processIds | ForEach-Object {
                $processId = $_
                $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
                $performance = $performanceRows | Where-Object {
                    [int]$_.IDProcess -eq $processId
                } | Select-Object -First 1
                if ($null -ne $process -and $null -ne $performance) {
                    [pscustomobject]@{
                        WorkingSetBytes = [double]$process.WorkingSet64
                        PrivateWorkingSetBytes = [double]$performance.WorkingSetPrivate
                        PrivateBytes = [double]$process.PrivateMemorySize64
                    }
                }
            })
        if ($processRows.Count -ne $processIds.Count) {
            throw 'A process left the A^3 tree while an idle-RAM sample was collected.'
        }

        [pscustomobject]@{
            ProcessCount = $processRows.Count
            TotalWorkingSetMiB = (($processRows.WorkingSetBytes | Measure-Object -Sum).Sum / 1MB)
            PrivateWorkingSetMiB = (($processRows.PrivateWorkingSetBytes | Measure-Object -Sum).Sum / 1MB)
            PrivateBytesMiB = (($processRows.PrivateBytes | Measure-Object -Sum).Sum / 1MB)
        }
        Start-Sleep -Seconds $SampleIntervalSeconds
    }

    $operatingSystem = Get-CimInstance Win32_OperatingSystem
    $processor = Get-CimInstance Win32_Processor | Select-Object -First 1
    [pscustomobject]@{
        schemaVersion = 1
        startedAtUtc = $startedAtUtc.ToString('O')
        executable = $resolvedExecutable
        modelServersExcluded = $true
        observedModelServers = $observedModelServers
        warmupSeconds = $WarmupSeconds
        samples = $SampleCount
        sampleIntervalSeconds = $SampleIntervalSeconds
        minProcessCount = ($samples.ProcessCount | Measure-Object -Minimum).Minimum
        maxProcessCount = ($samples.ProcessCount | Measure-Object -Maximum).Maximum
        idlePrivateWorkingSetMedianMiB = [math]::Round(
            (Get-Median -Values $samples.PrivateWorkingSetMiB),
            3
        )
        sampledPrivateWorkingSetPeakMiB = [math]::Round(
            (($samples.PrivateWorkingSetMiB | Measure-Object -Maximum).Maximum),
            3
        )
        idleTotalWorkingSetMedianMiB = [math]::Round(
            (Get-Median -Values $samples.TotalWorkingSetMiB),
            3
        )
        sampledTotalWorkingSetPeakMiB = [math]::Round(
            (($samples.TotalWorkingSetMiB | Measure-Object -Maximum).Maximum),
            3
        )
        idlePrivateBytesMedianMiB = [math]::Round(
            (Get-Median -Values $samples.PrivateBytesMiB),
            3
        )
        sampledPrivateBytesPeakMiB = [math]::Round(
            (($samples.PrivateBytesMiB | Measure-Object -Maximum).Maximum),
            3
        )
        platform = [pscustomobject]@{
            operatingSystem = $operatingSystem.Caption
            build = $operatingSystem.BuildNumber
            processor = $processor.Name.Trim()
            physicalCores = $processor.NumberOfCores
            logicalProcessors = $processor.NumberOfLogicalProcessors
            physicalMemoryGiB = [math]::Round(
                ([double]$operatingSystem.TotalVisibleMemorySize / 1MB),
                3
            )
        }
    } | ConvertTo-Json -Depth 3
}
finally {
    $processTable = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $processIds = @(Get-ProcessTreeIds -RootProcessId $app.Id -ProcessTable $processTable)
    foreach ($processId in ($processIds | Where-Object { $_ -ne $app.Id })) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    if (-not $app.HasExited) {
        Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue
    }
}
