#![deny(clippy::pedantic)]

use std::env::args;

use anyhow::Result;
use log::error;
use tokio::{
    fs::File,
    io::{stdout, BufReader},
};

use self::csv::parse_transactions;

#[macro_use]
extern crate serde;

mod csv;
mod model;
mod transaction;

#[actix::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let filename = args()
        .nth(1)
        .expect("The filemane should be specified as the first parameter");
    let csv_file = File::open(filename)
        .await
        .expect("Could not open specified file");

    let buf_reader = BufReader::new(csv_file);
    if let Err(e) = parse_transactions(buf_reader, stdout()).await {
        error!("Error processing file: {e}");
    }
    Ok(())
}
