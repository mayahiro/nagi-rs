//! Shows a transient spinner and determinate progress with plain-log fallback

use std::error::Error;
use std::thread;
use std::time::Duration;

use nagi_cli_status::{ProcessIo, Reporter, Snapshot};

fn main() -> Result<(), Box<dyn Error>> {
    let mut reporter = Reporter::new(ProcessIo::default());

    for tick in 0..8 {
        reporter.update(Snapshot::spinner(tick, "Preparing"))?;
        thread::sleep(Duration::from_millis(60));
    }

    for completed in 0..=4 {
        reporter.update(Snapshot::progress(completed, 4, "Building"))?;
        thread::sleep(Duration::from_millis(60));
    }
    reporter.finish(Snapshot::progress(4, 4, "Complete"))?;
    Ok(())
}
