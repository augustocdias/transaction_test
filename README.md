# Transactions test

## Building and running

To build this crate, simply run `cargo build`

The executable expects the first parameter to be the `csv` filename: `cargo run -- transactions.csv`

The result will be printed in the std out.

Unit tests can be ran with `cargo test`.

## Assumptions

The values will be rounded to 4 digits using the `Bankers Rounding` strategy (when a number is halfway between two others, it is rounded toward the nearest even number. e.g. 6.5 -> 6, 7.5 -> 8).

My assumption is that a `Withdrawal` cannot be disputed, because the money is already taken away.
An error will be logged when that happens.

Errors and warnings will be logged in the std err. No error will block the application from
continuing. All errors are provenient of invalid transactions because of business rules.

Since transaction not found shouldn't be treated as an error, it will be logged as a warning only.

Some errors are not recoverable, such as IO errors. They are handled and logged, but the
application stops when they happen. Lines with deserialization errors, are ignored.

Not passing the file or a file that cannot be opened will result in a panic.

Although I haven't tested it with a huge file, it should work fine as the file is streamed and not
read entierely into memory.

## Implementation Details

The program runs on top of `actix` runtime which runs on top of `tokio`.

Every time a new client is found in the transactions file, a new `actor` is created (it essentialy
translates to a future task). It will be awaken when messages are received. Other than memory it
shouldn't consume any resource if it isn't processing any message.

At the end of the program, the current state of all actors are collected and written to the std out
in csv format.
