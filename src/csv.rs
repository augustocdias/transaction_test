use std::collections::HashMap;

use anyhow::Result;
use csv_async::Trim::All;
use csv_async::{AsyncReaderBuilder, AsyncSerializer};
use log::{error, warn};
use tokio::io::{AsyncBufRead, AsyncWrite, BufWriter};
use tokio_stream::StreamExt;

use crate::model::{Collect, Transaction, TransactionError};
use crate::transaction::AccountHandler;

/// Parse the transactions of the provided reader and outputs the accounts into the provided writer
pub async fn parse_transactions(
    buf_reader: impl AsyncBufRead + Send + Unpin,
    buf_writer: impl AsyncWrite + Unpin,
) -> Result<()> {
    let mut csv_reader = AsyncReaderBuilder::new()
        .has_headers(true)
        .delimiter(b',')
        .trim(All)
        .create_deserializer(buf_reader);
    let mut client_accounts = HashMap::new();
    let mut record_stream = csv_reader.deserialize::<Transaction>();
    while let Some(record) = record_stream.next().await {
        let transaction = match record {
            Ok(t) => t,
            Err(e) => {
                error!("Could not parse line: {e}");
                continue;
            }
        };
        let actor = client_accounts
            .entry(transaction.client)
            .or_insert_with(|| AccountHandler::new(transaction.client));
        if let Err(e) = actor.send(transaction).await? {
            match e {
                TransactionError::InsufficientFunds => error!("Insuficient funds"),
                TransactionError::InvalidOperation => error!("Invalid opertation"),
                TransactionError::AccountLocked => error!("Account locked"),
                TransactionError::TransactionAlreadyInDispute => {
                    error!("Transaction already in dispute");
                }
                TransactionError::TransactionNotInDispute => error!("Transaction not in dispute"),
                TransactionError::TransactionNotFound => warn!("Transaction not found"),
            }
        }
    }

    let buf_writer = BufWriter::new(buf_writer);
    let mut serializer = AsyncSerializer::from_writer(buf_writer);
    for (client, actor) in client_accounts {
        match actor.send(Collect).await {
            Ok(account) => {
                serializer.serialize(account).await?;
            }
            Err(e) => {
                error!("Could not collect account data from client {client}: {e}");
            }
        }
    }

    Ok(())
}
