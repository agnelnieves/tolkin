use anyhow::Result;
use clap::Args;

use crate::onboard;

#[derive(Args, Debug)]
pub struct InitArgs {}

pub fn run(_args: InitArgs, yes: bool) -> Result<()> {
    onboard::run(true, yes)
}
