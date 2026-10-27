use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use courage_core::{
    CaptureEngine, CaptureSource, ClipEditor, ClipExporter, ExportOptions, ExportQueue,
    SyntheticSource, VideoSpec,
};
use courage_store::ClipStore;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("demo") => run_demo(args.collect())?,
        Some("trim") => run_trim(args.collect())?,
        _ => print_usage(),
    }
    Ok(())
}

fn print_usage() {
    println!(
        "Usage: courage-cli demo [--output PATH] [--seconds N] [--replay-seconds N] [--fps N] [--no-audio]\n       courage-cli trim --input PATH --output PATH --start SECONDS --end SECONDS"
    );
}

fn run_trim(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let input = value(&args, "--input").ok_or("--input is required")?;
    let output = value(&args, "--output").ok_or("--output is required")?;
    let start = value(&args, "--start").ok_or("--start is required")?.parse::<f64>()?;
    let end = value(&args, "--end").ok_or("--end is required")?.parse::<f64>()?;
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end < 0.0 {
        return Err("trim times must be finite and non-negative".into());
    }
    let report = ClipEditor::trim(
        input,
        &output,
        Duration::from_secs_f64(start),
        Duration::from_secs_f64(end),
    )?;
    println!("output={}", report.path.display());
    println!("duration_seconds={:.3}", report.duration_seconds);
    println!("output_bytes={}", report.bytes);
    Ok(())
}

fn run_demo(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let output = value(&args, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/courage-demo.mp4"));
    let seconds = value(&args, "--seconds").unwrap_or_else(|| "4".to_owned()).parse::<u32>()?;
    let replay_seconds =
        value(&args, "--replay-seconds").unwrap_or_else(|| seconds.to_string()).parse::<u64>()?;
    let fps = value(&args, "--fps").unwrap_or_else(|| "24".to_owned()).parse::<u32>()?;
    if seconds == 0 || replay_seconds == 0 {
        return Err("seconds and replay-seconds must be greater than zero".into());
    }

    let spec = VideoSpec::new(640, 360, fps)?;
    let frame_count = seconds as usize * fps as usize;
    let capacity = replay_seconds.saturating_mul(fps as u64).max(1) as usize;
    let max_bytes = spec.frame_bytes().saturating_mul(capacity).max(spec.frame_bytes());
    let engine = CaptureEngine::new(spec, capacity, max_bytes)?;
    let started = Instant::now();
    let mut source = SyntheticSource::new(spec);
    for _ in 0..frame_count {
        let frame = source.next_frame()?.ok_or("synthetic source ended")?;
        engine.push_frame(frame)?;
    }
    let capture_elapsed = started.elapsed();
    let snapshot = engine.snapshot(Duration::from_secs(replay_seconds))?;
    let queue = ExportQueue::new(ClipExporter::new(spec), 1)?;
    let ticket = queue.try_submit(
        snapshot,
        &output,
        ExportOptions {
            include_synthetic_audio: !args.iter().any(|arg| arg == "--no-audio"),
            ..ExportOptions::default()
        },
    )?;
    let report = ticket.wait()?;
    let export_elapsed = started.elapsed().saturating_sub(capture_elapsed);

    let database_path = output.with_extension("sqlite");
    let store = ClipStore::open(&database_path)?;
    store.insert_clip(&report.path, (report.duration_seconds * 1000.0) as i64, None)?;
    let stats = engine.stats()?;
    println!("capture_frames={frame_count}");
    println!("buffer_frames={}", stats.frame_count);
    println!("buffer_bytes={}", stats.byte_count);
    println!("evicted_frames={}", stats.evicted_frames);
    println!("capture_seconds={:.3}", capture_elapsed.as_secs_f64());
    println!("export_seconds={:.3}", export_elapsed.as_secs_f64());
    println!("output={}", report.path.display());
    println!("output_bytes={}", report.bytes);
    println!("metadata={}", database_path.display());
    Ok(())
}

fn value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|pair| pair[0] == name).map(|pair| pair[1].clone())
}
