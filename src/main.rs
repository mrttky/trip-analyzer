use clap::{App, Arg};
use std::error::Error;
use serde::{Deserialize, Serialize};
use chrono::prelude::*;
use hdrhistogram::Histogram;


struct DurationHistograms(Vec<Histogram<u64>>);

impl DurationHistograms {
    fn new() -> AppResult<Self> {
        let lower_bound = 1;
        let upper_bound = 3 * 60 * 60;
        let hist = Histogram::new_with_bounds(lower_bound, upper_bound, 3)
            .map_err(|e| format!("{:?}", e))?;
        let histograms = std::iter::repeat(hist).take(24).collect();
        Ok(Self(histograms))
    }
    fn record_duration(&mut self, pickup: DT, dropoff: DT) -> AppResult<()> {
        let duration = (dropoff - pickup).num_seconds() as u64;
        if duration < 20 * 60 {
            Err(format!("duration secs {} is too short.", duration).into())
        } else {
            let hour = pickup.hour() as usize;
            self.0[hour]
                .record(duration)
                .map_err(|e| {
                    format!("duration secs {} is too long. {:?}", duration, e)
                        .into()
                })
        }
    }
}

type DT = NaiveDateTime;
type AppResult<T> = Result<T, Box<dyn Error>>;

fn parse_datetime(s: &str) -> AppResult<DT> {
    DT::parse_from_str(s, "%Y-%m-%d %H:%M:%S").map_err(|e| e.into())
}

fn is_in_middtown(loc: LocId) -> bool {
    let locations = [90, 100, 161, 162, 163, 164, 186, 230, 234];
    locations.binary_search(&loc).is_ok()
}

fn is_jfk_airport(loc: LocId) -> bool {
    loc == 132
}

fn is_weekday(datetime: DT) -> bool {
    datetime.weekday().number_from_monday() <= 5
}

type LocId = u16;
#[derive(Debug, Deserialize)]
struct Trip {
    #[serde(rename = "tpep_pickup_datetime")]
    pickup_datetime: String,
    #[serde(rename = "tpep_dropoff_datetime")]
    dropoff_datetime: String,
    #[serde(rename = "PULocationID")]
    pickup_loc: LocId,
    #[serde(rename = "DOLocationID")]
    dropoff_loc: LocId,
}

#[derive(Debug, Serialize)]
struct RecordCounts {
    read: u32,
    matched: u32,
    skipped:u32,
}

impl Default for RecordCounts {
    fn default() -> Self {
        Self {
            read: 0,
            matched: 0,
            skipped: 0,
        }
    }
}

#[derive(Serialize)]
struct DisplayStats {
    record_counts: RecordCounts,
    stats: Vec<StatsEntry>,
}

#[derive(Serialize)]
struct StatsEntry {
    hour_of_day: u8,
    minimum: f64,
    median: f64,
    #[serde(rename = "95th percentile")]
    p95: f64,
}

impl DisplayStats {
    fn new(record_counts: RecordCounts, histograms: DurationHistograms) -> Self {
        let stats = histograms.0.iter().enumerate()
            .map(|(i, hist)| StatsEntry {
                hour_of_day: i as u8,
                minimum: hist.min() as f64 / 60.0,
                median: hist.value_at_quantile(0.5) as f64 / 60.0,
                p95: hist.value_at_quantile(0.95) as f64 / 60.0,
            })
            .collect();
        Self {
            record_counts,
            stats,
        }
    }
}

fn analyze(infile: &str) -> AppResult<String> {
    let mut reader = csv::Reader::from_path(infile)?;
    let mut rec_counts = RecordCounts::default();
    let mut hist = DurationHistograms::new()?;
    for (i, result) in reader.deserialize().enumerate() {
        let trip: Trip = result?;
        rec_counts.read += 1;
        if is_jfk_airport(trip.dropoff_loc) && is_in_middtown(trip.pickup_loc) {
            let pickup = parse_datetime(&trip.pickup_datetime)?;
            if is_weekday(pickup) {
                rec_counts.matched += 1;
                let dropoff = parse_datetime(&trip.dropoff_datetime)?;
                hist.record_duration(pickup, dropoff)
                    .unwrap_or_else(|e| {
                        eprintln!("WARN: {} - {}. Skipped: {:?}", i + 2, e, trip);
                        rec_counts.skipped += 1;
                    });
            }
        }
    }
    println!("{:?}", rec_counts);
    let display_stats = DisplayStats::new(rec_counts, hist);
    let json = serde_json::to_string_pretty(&display_stats)?;
    Ok(json)
}




fn main() {
    let arg_matches = App::new("trip-analyzer")
        .version("1.0")
        .about("Analyzer")
        .arg(Arg::with_name("INFILE")
             .help("Sets the input CSV file")
             .index(1)
             .required(true)
        ) 
        .get_matches();
    let infile = arg_matches.value_of("INFILE").unwrap();
    match analyze(infile) {
        Ok(json) => println!("{}", json),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
