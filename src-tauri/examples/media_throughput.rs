use std::{
    env,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use trans_kun_lib::media::{decode_mono_16khz, probe, ChunkOptions, Chunker};

const WORKERS: usize = 4;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let source = args
        .next()
        .map(PathBuf::from)
        .ok_or("usage: media_throughput <media-file> [iterations-per-worker]")?;
    let iterations = args
        .next()
        .map(|value| value.to_string_lossy().parse::<usize>())
        .transpose()?
        .unwrap_or(3);
    if iterations == 0 || args.next().is_some() {
        return Err("iterations must be positive; no extra arguments are accepted".into());
    }

    let input = probe(&source)?;
    let started = Instant::now();
    let handles = (0..WORKERS)
        .map(|_| {
            let source = source.clone();
            thread::spawn(move || -> Result<(f64, Duration, usize), String> {
                let worker_started = Instant::now();
                let mut decoded_seconds = 0.0;
                let mut chunks_emitted = 0;
                for _ in 0..iterations {
                    let mut chunker =
                        Chunker::new(ChunkOptions::default()).map_err(|error| error.to_string())?;
                    let mut emit_chunk = |_| {
                        chunks_emitted += 1;
                        Ok(())
                    };
                    let stats = decode_mono_16khz(&source, |samples| {
                        chunker.push(samples, &mut emit_chunk)
                    })
                    .map_err(|error| error.to_string())?;
                    chunker
                        .finish(&mut emit_chunk)
                        .map_err(|error| error.to_string())?;
                    decoded_seconds += stats.duration_seconds;
                }
                Ok((decoded_seconds, worker_started.elapsed(), chunks_emitted))
            })
        })
        .collect::<Vec<_>>();

    let mut results = Vec::with_capacity(WORKERS);
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| std::io::Error::other("benchmark worker panicked"))?;
        results.push(result.map_err(std::io::Error::other)?);
    }
    let wall = started.elapsed();
    let decoded_seconds = results.iter().map(|(seconds, _, _)| seconds).sum::<f64>();
    let chunks_emitted = results.iter().map(|(_, _, chunks)| chunks).sum::<usize>();
    let aggregate_realtime = decoded_seconds / wall.as_secs_f64();
    let worker_realtime = results
        .iter()
        .map(|(seconds, elapsed, _)| seconds / elapsed.as_secs_f64())
        .collect::<Vec<_>>();

    println!("source={}", source.display());
    println!("source_container={}", input.container);
    println!("source_duration_seconds={:.6}", input.duration_seconds);
    println!("workers={WORKERS}");
    println!("iterations_per_worker={iterations}");
    println!("decoded_media_seconds_total={decoded_seconds:.6}");
    println!("flac_chunks_emitted={chunks_emitted}");
    println!("wall_seconds={:.6}", wall.as_secs_f64());
    println!("aggregate_realtime_x={aggregate_realtime:.2}");
    println!(
        "per_worker_realtime_x={}",
        worker_realtime
            .iter()
            .map(|value| format!("{value:.2}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("workers are concurrent threads; this does not pin or cap physical CPU cores");

    Ok(())
}
