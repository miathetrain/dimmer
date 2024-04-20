use anyhow::{Context, Result};
use glob::{glob, Paths};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use structopt::StructOpt;
use thiserror::Error;

#[derive(Error, Debug)]
enum DimmerError {
    #[error("Invalid percentage given by user")]
    InvalidPercentage,
    #[error("Failed to parse invalid Brightness")]
    InvalidBrightness(#[from] std::num::ParseIntError),
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
struct Brightness(u64);

impl std::fmt::Display for Brightness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for Brightness {
    type Err = DimmerError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Ok(input.parse::<u64>().map(Brightness)?)
    }
}

impl Brightness {
    fn parse_with_percentage(input: &str, max: Brightness) -> Result<Brightness> {
        match input.strip_suffix('%') {
            Some(percentage) => {
                let percentage = percentage.parse::<u64>()?;
                if percentage > 100 {
                    return Err(DimmerError::InvalidPercentage.into());
                }
                Ok(Brightness(
                    ((percentage as f64 / 100.0) * max.0 as f64) as u64,
                ))
            }
            None => Ok(input.parse::<u64>().map(Brightness)?),
        }
    }

    fn from_file<P: AsRef<Path>>(path: P) -> Result<Brightness> {
        let path = path.as_ref();
        let res = std::fs::read_to_string(path)
            .context("Failed to read {path}")?
            .trim()
            .parse()
            .context("Failed to parse brightness from {path}")?;
        Ok(res)
    }
}

#[derive(Debug, StructOpt)]
struct Opt {
    #[structopt(long, short)]
    restore: bool,
}

const SYS_BACKLIGHT_PREFIX: &str = "/sys/class/backlight";

fn main() -> Result<()> {
    let opt = Opt::from_args();

    let glob_path = format!("{SYS_BACKLIGHT_PREFIX}/*/brightness");
    let glob: Paths = glob(&glob_path).expect("Failed to read glob pattern");

    let mut thread = None;
    for i in glob {
        let parent = i.unwrap().parent().unwrap().to_str().unwrap().to_owned();

        let q = parent.clone() + "/brightness";
        let w = parent.clone() + "/actual_brightness";
        let e = parent.clone() + "/max_brightness";
        let brightness_file = Path::new(&q);
        let current_brightness_file = Path::new(&w);
        let max_brightness_file = Path::new(&e);

        let stored: Brightness = Brightness::from_file(&current_brightness_file)?;
        let maximum: Brightness = Brightness::from_file(&max_brightness_file)?;

        let target: Brightness = if opt.restore {
            if parent == "{SYS_BACKLIGHT_PREFIX}/ddcci9" {
                Brightness::parse_with_percentage("70", maximum)?
            } else {
                Brightness::parse_with_percentage("100", maximum)?
            }
        } else {
            Brightness::parse_with_percentage("0", maximum)?
        };

        let target = if target > maximum { maximum } else { target };

        let step_size = 4;

        let output = Arc::new(Mutex::new(File::create(&brightness_file)?));
        let mut brightness = stored;

        let file = Arc::clone(&output);

        thread = Some(thread::spawn(move || loop {
            if target.0 == brightness.0 {
                break;
            }
            if target.0 == 0 {
                if brightness.0 < step_size {
                    brightness = Brightness(0);
                } else {
                    brightness = Brightness(brightness.0 - step_size);
                }
            } else if (target.0 - brightness.0) < step_size {
                brightness = target;
            } else {
                brightness = target;
            }

            dbg!(&output);
            let mut file = file.lock().unwrap();
            write!(file, "{}", brightness.0).expect("Failed to write file!");
            std::thread::sleep(std::time::Duration::from_millis(1000 / 100));
        }));
    }

    if let Some(value) = thread {
        let _ = value.join();
    }
    println!("Ok!");
    Ok(())
}
