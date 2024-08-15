// Copyright (C) 2024  DCsunset

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

mod transformfs;
mod utils;
mod output;

use transformfs::TransformFs;
use std::{path::PathBuf, time::Duration};
use daemonize::Daemonize;
use clap::Parser;
use fuser::{self, MountOption};

#[derive(Parser)]
#[command(version)]
struct Args {
  /// Mount point of the target transformfs
  mount_point: PathBuf,

  /// The input dirs/files to pass to transform function
  #[arg(short, long)]
  input: Vec<PathBuf>,

  /// script
  #[arg(short, long)]
  script: PathBuf,

  /// Allow other users to access the mounted fs
  #[arg(long)]
  allow_other: bool,

  /// Allow root user to access the mounted fs
  #[arg(long)]
  allow_root: bool,

  /// Time to live for metadata and cache in seconds
  #[arg(short, long, default_value_t = 1)]
  ttl: u64,

  /// Unmount automatically when program exists.
  /// (need --allow-root or --allow-other; auto set one if not specified)
  #[arg(short, long)]
  auto_unmount: bool,

  /// Run in foreground
  #[arg(long)]
  foreground: bool,

  /// Redirect stdout to file (only when in background)
  #[arg(long)]
  stdout: Option<PathBuf>,

  /// Redirect stderr to file (only when in background)
  #[arg(long)]
  stderr: Option<PathBuf>
}


fn main() -> anyhow::Result<()> {
  env_logger::init();
  let args = Args::parse();
  let mut options = vec![
    MountOption::RO,
    MountOption::FSName("transformfs".to_string()),
    MountOption::Subtype("transformfs".to_string()),
  ];
  if args.allow_other {
    options.push(MountOption::AllowOther);
  }
  if args.allow_root {
    options.push(MountOption::AllowRoot);
  }
  if args.auto_unmount {
    options.push(MountOption::AutoUnmount);
  }

  if !args.foreground {
    let mut daemon = Daemonize::new().working_directory(".");
    if let Some(stdout) = args.stdout {
      daemon = daemon.stdout(std::fs::File::create(stdout)?);
    }
    if let Some(stderr) = args.stderr {
      daemon = daemon.stderr(std::fs::File::create(stderr)?);
    }
    daemon.start()?;
  }

  fuser::mount2(
    TransformFs::init(args.input, args.script, Duration::from_secs(args.ttl))?,
    args.mount_point,
    &options
  )?;

  Ok(())
}
