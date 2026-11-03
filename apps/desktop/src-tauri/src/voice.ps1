$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
function Send-Event($value) {
    [Console]::WriteLine(($value | ConvertTo-Json -Compress))
    [Console]::Out.Flush()
}
$recognizer = $null
try {
    $config = [Console]::ReadLine() | ConvertFrom-Json
    Add-Type -AssemblyName System.Speech
    # Console.In.ReadLineAsync can block synchronously on .NET Framework's
    # synchronized TextReader. Use one background stdin reader instead.
    Add-Type -TypeDefinition @'
using System;
using System.Threading;
public static class CourageStop {
    public static volatile bool Requested;
    public static void Start() {
        new Thread(() => { try { Console.ReadLine(); } catch (System.IO.IOException) { } finally { Requested = true; } }) { IsBackground = true }.Start();
    }
}
'@
    $installed = @([System.Speech.Recognition.SpeechRecognitionEngine]::InstalledRecognizers())
    $info = $installed | Where-Object { $_.Culture.Name -eq $config.locale } | Select-Object -First 1
    if (-not $info) {
        throw "No System.Speech recognizer for $($config.locale). Installed: $(($installed | ForEach-Object { $_.Culture.Name }) -join ', ')"
    }
    $recognizer = New-Object System.Speech.Recognition.SpeechRecognitionEngine($info)
    $wake = $config.wakePhrase.Trim()
    $fullBuilder = New-Object System.Speech.Recognition.GrammarBuilder
    $fullBuilder.Culture = $info.Culture
    $fullBuilder.Append($wake)
    if (-not $config.wakeOnly) {
        # Build the command as an ordered grammar instead of one free-form
        # string. This tolerates a natural pause after the wake phrase.
        $fullBuilder.Append($config.commandPhrase.Trim())
    }
    $recognizer.LoadGrammar((New-Object System.Speech.Recognition.Grammar($fullBuilder)))
    if (-not $config.wakeOnly) {
        # Emit isolated wake/command results too; Rust uses them for the
        # short two-step flow while still requiring the exact configured words.
        $diagnosticChoices = New-Object System.Speech.Recognition.Choices
        $diagnosticChoices.Add($wake)
        $diagnosticChoices.Add($config.commandPhrase.Trim())
        $diagnosticBuilder = New-Object System.Speech.Recognition.GrammarBuilder
        $diagnosticBuilder.Culture = $info.Culture
        $diagnosticBuilder.Append($diagnosticChoices)
        $recognizer.LoadGrammar((New-Object System.Speech.Recognition.Grammar($diagnosticBuilder)))
    }
    $recognizer.MaxAlternates = 3
    $recognizer.InitialSilenceTimeout = [TimeSpan]::FromMilliseconds(500)
    $recognizer.BabbleTimeout = [TimeSpan]::FromSeconds(4)
    $recognizer.EndSilenceTimeout = [TimeSpan]::FromMilliseconds(500)
    $recognizer.EndSilenceTimeoutAmbiguous = [TimeSpan]::FromMilliseconds(800)
    $recognizer.SetInputToDefaultAudioDevice()
    $format = $recognizer.AudioFormat
    Send-Event @{ kind='ready'; locale=$info.Culture.Name; microphone='Windows default recording device (System.Speech)'; audioFormat="$($format.SamplesPerSecond) Hz / $($format.ChannelCount) channels / $($format.BitsPerSample) bits" }
    [CourageStop]::Start()
    $id = 0
    while (-not [CourageStop]::Requested) {
        # Synchronous recognition returns final results only; hypotheses never trigger.
        $result = $recognizer.Recognize([TimeSpan]::FromMilliseconds(500))
        if ($result -and -not [CourageStop]::Requested) {
            $id++
            Send-Event @{ kind='result'; id=$id; phrase=$result.Text; confidence=$result.Confidence; finalResult=$true }
        }
    }
} catch {
    Send-Event @{ kind='error'; error=$_.Exception.Message }
    exit 1
} finally {
    if ($recognizer) {
        try { $recognizer.SetInputToNull() } finally { $recognizer.Dispose() }
    }
}
