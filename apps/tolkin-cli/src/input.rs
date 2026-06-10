use std::io::{self, Read};
use std::path::Path;

use anyhow::{Context, Result};

use crate::parse;

/// Read input text from a file path or stdin. Treat `None` or a path equal to
/// `-` as a request to read stdin. Real file paths go through `parse::extract`
/// so binary documents (PDF, DOCX, XLSX) yield the text a model would receive.
pub fn read<P: AsRef<Path>>(file: Option<P>) -> Result<String> {
    match file {
        Some(path) if path.as_ref().as_os_str() == "-" => read_stdin(),
        None => read_stdin(),
        Some(path) => parse::extract(path.as_ref()),
    }
}

fn read_stdin() -> Result<String> {
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .context("failed to read stdin")?;
    Ok(buf)
}
