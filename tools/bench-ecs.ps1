param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("Before", "After", "Run")]
    [string]$Phase,

    [string]$Baseline = "ecs-change",
    [ValidateRange(1, 9)]
    [int]$Rounds = 3,
    [ValidateRange(0, 60)]
    [int]$CooldownSeconds = 5,
    [switch]$IncludeParallel,
    [string[]]$Only
)

$ErrorActionPreference = "Stop"
$env:RAYON_NUM_THREADS = "8"

$benchmarks = @(
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_cache"; Name = "prepared_epoch_hit" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_fresh"; Name = "query_1_staggered_8" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_fresh"; Name = "query_8_early_reject" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_fresh"; Name = "query_16_dense_8" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_refresh"; Name = "append_matching_one" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_refresh"; Name = "append_nonmatching_one" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_filter"; Name = "single_selective_with" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_filter"; Name = "and_redundant_7" },
    [pscustomobject]@{ Target = "archetype_match"; Group = "archetype_filter"; Name = "any_fallback" },
    [pscustomobject]@{ Target = "bound_query"; Group = "bound_query"; Name = "world_cache_hit" },
    [pscustomobject]@{ Target = "parallel_job_cache"; Group = "parallel_job_cache"; Name = "rebuild_after_spawn_despawn_100k" }
)

if ($IncludeParallel) {
    $benchmarks += @(
        [pscustomobject]@{ Target = "parallel_query"; Group = "parallel_query_bound_facade"; Name = "tuple_parallel" },
        [pscustomobject]@{ Target = "parallel_query"; Group = "parallel_query_bound_facade"; Name = "tuple_parallel_chunk" }
    )
}

if ($Only.Count -gt 0) {
    $selected = @($benchmarks | Where-Object { "$($_.Group)/$($_.Name)" -in $Only })
    $missing = @($Only | Where-Object { $_ -notin @($selected | ForEach-Object { "$($_.Group)/$($_.Name)" }) })
    if ($missing.Count -gt 0) {
        throw "unknown benchmark ID(s): $($missing -join ', ')"
    }
    $benchmarks = $selected
}

function Get-Median([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 0) {
        return 0.0
    }
    $middle = [int][Math]::Floor($sorted.Count / 2)
    if ($sorted.Count % 2 -eq 1) {
        return [double]$sorted[$middle]
    }
    return ([double]$sorted[$middle - 1] + [double]$sorted[$middle]) / 2.0
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$outputDirectory = Join-Path "target" "criterion"
New-Item -ItemType Directory -Force -Path $outputDirectory | Out-Null

$cpuName = $env:PROCESSOR_IDENTIFIER
try {
    $detectedCpu = Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name
    if ($detectedCpu) {
        $cpuName = $detectedCpu
    }
} catch {
    # PROCESSOR_IDENTIFIER remains the portable fallback.
}

$metadata = [ordered]@{
    phase = $Phase
    baseline = $Baseline
    rounds = $Rounds
    cooldown_seconds = $CooldownSeconds
    rayon_threads = 8
    timestamp = (Get-Date).ToString("o")
    cpu = $cpuName
    logical_processors = $env:NUMBER_OF_PROCESSORS
    os = $PSVersionTable.OS
    rustc = ((& rustc -Vv) -join "`n")
    git_revision = ((& git rev-parse HEAD) -join "").Trim()
    dirty_worktree = [bool]((& git status --porcelain | Select-Object -First 1))
    benchmarks = @($benchmarks | ForEach-Object { "$($_.Group)/$($_.Name)" })
}

$metadataPath = Join-Path $outputDirectory "sky-ecs-$($Phase.ToLower())-$timestamp-metadata.json"
$metadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $metadataPath -Encoding utf8

$results = @()
for ($round = 1; $round -le $Rounds; $round++) {
    foreach ($benchmark in $benchmarks) {
        $filter = "$($benchmark.Group)/$($benchmark.Name)"
        $arguments = @("bench", "--bench", $benchmark.Target, "--", $filter, "--noplot")
        switch ($Phase) {
            "Before" {
                $arguments += @("--save-baseline", "$Baseline-before-$round")
            }
            "After" {
                $arguments += @(
                    "--baseline", "$Baseline-before-$round",
                    "--save-baseline", "$Baseline-after-$round"
                )
            }
            "Run" {
                $arguments += @("--save-baseline", "$Baseline-run-$round")
            }
        }

        Write-Host "[$Phase round $round/$Rounds] cargo $($arguments -join ' ')"
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) {
            throw "benchmark failed: $filter"
        }

        $estimatePath = Join-Path $outputDirectory "$($benchmark.Group)/$($benchmark.Name)/new/estimates.json"
        if (-not (Test-Path -LiteralPath $estimatePath)) {
            throw "Criterion estimate missing: $estimatePath"
        }
        $estimate = Get-Content -LiteralPath $estimatePath -Raw | ConvertFrom-Json
        $results += [pscustomobject]@{
            round = $round
            target = $benchmark.Target
            benchmark = $filter
            median_ns = [double]$estimate.median.point_estimate
            lower_ns = [double]$estimate.median.confidence_interval.lower_bound
            upper_ns = [double]$estimate.median.confidence_interval.upper_bound
        }

        if ($CooldownSeconds -gt 0) {
            Start-Sleep -Seconds $CooldownSeconds
        }
    }
}

$summary = @($results | Group-Object benchmark | ForEach-Object {
    $values = @($_.Group | ForEach-Object { [double]$_.median_ns })
    [pscustomobject]@{
        benchmark = $_.Name
        median_of_medians_ns = Get-Median $values
        rounds = $values
    }
})

$report = [ordered]@{
    metadata = $metadata
    samples = $results
    summary = $summary
}
$reportPath = Join-Path $outputDirectory "sky-ecs-$($Phase.ToLower())-$timestamp-results.json"
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $reportPath -Encoding utf8

$markdown = @(
    "# Sky ECS benchmark run",
    "",
    "- Phase: $Phase",
    "- Baseline: $Baseline",
    "- Rounds: $Rounds",
    "- RAYON_NUM_THREADS: 8",
    "- Metadata: ``$metadataPath``",
    "",
    "| Benchmark | Median of medians (ns) | Round medians (ns) |",
    "|---|---:|---|"
)
foreach ($entry in $summary) {
    $roundValues = ($entry.rounds | ForEach-Object { "{0:N3}" -f $_ }) -join ", "
    $markdown += "| ``$($entry.benchmark)`` | $([string]::Format('{0:N3}', $entry.median_of_medians_ns)) | $roundValues |"
}
$markdownPath = Join-Path $outputDirectory "sky-ecs-$($Phase.ToLower())-$timestamp-results.md"
$markdown -join "`n" | Set-Content -LiteralPath $markdownPath -Encoding utf8

Write-Host "Metadata: $metadataPath"
Write-Host "Results:  $reportPath"
Write-Host "Summary:  $markdownPath"
