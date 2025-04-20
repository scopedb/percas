// Copyright 2025 ScopeDB <contact@scopedb.io>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(addr_parse_ascii)]
#![feature(ip)]

use clap::Parser;
use error_stack::Result;
use percas_styled::styled;
use percas_version::version;
use thiserror::Error;

mod start;

#[derive(Debug, clap::Parser)]
#[command(name = "percas", version, long_version = version(), styles=styled())]
struct Command {
    #[clap(subcommand)]
    cmd: SubCommand,
}

impl Command {
    pub fn run(self) -> Result<(), Error> {
        match self.cmd {
            SubCommand::Start(cmd) => cmd.run(),
        }
    }
}

#[derive(Debug, clap::Subcommand)]
enum SubCommand {
    /// Start a Percas node.
    Start(start::CommandStart),
}

#[derive(Debug, Error)]
#[error("{0}")]
struct Error(String);

fn main() -> Result<(), Error> {
    let cmd = Command::parse();
    cmd.run()
}
