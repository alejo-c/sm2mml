use std::{
    fs,
    io::{self, IsTerminal, Read},
    path::PathBuf,
};

use anyhow::Result;
use clap::Parser;

use sm2mml::starmath_to_mathml;

#[derive(Parser)]
struct CLI {
    text: Option<PathBuf>,
}

fn main() -> Result<()> {
    let content = if let Some(text) = CLI::parse().text {
        text
    } else {
        if io::stdin().is_terminal() {
            anyhow::bail!("No input provided. Use -f <file> or pipe StarMath expression.");
        }
        let mut content = String::new();
        io::stdin().read_to_string(&mut content)?;
        content
    };

    let output = starmath_to_mathml(&content)?;
    println!("{}", output);
    Ok(())
}
