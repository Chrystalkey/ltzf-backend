# LTZF-Backend

# WE HAVE MOVED

This project is now hosted here: https://codeberg.org/PaZuFa/parlamentszusammenfasser

And this repository is being archived.

## Arguments for LTZF-Backend
```bash
Usage: ltzf-backend.exe [OPTIONS] --db-url <DB_URL> --keyadder-key <KEYADDER_KEY>

Options:
      --mail-server <MAIL_SERVER>
          [env: MAIL_SERVER=]
      --mail-user <MAIL_USER>
          [env: MAIL_USER=]
      --mail-password <MAIL_PASSWORD>
          [env: MAIL_PASSWORD=]
      --mail-sender <MAIL_SENDER>
          [env: MAIL_SENDER=]
      --mail-recipient <MAIL_RECIPIENT>
          [env: MAIL_RECIPIENT=]
      --host <HOST>
          [env: LTZF_HOST=] [default: 0.0.0.0]
      --port <PORT>
          [env: LTZF_PORT=] [default: 80]
  -d, --db-url <DB_URL>
          [env: DATABASE_URL=postgres://ltzf-user:ltzf-pass@localhost:5432/ltzf]
  -c, --config <CONFIG>

      --keyadder-key <KEYADDER_KEY>
          The API Key that is used to add new Keys. This is saved in the database. [env: LTZF_KEYADDER_KEY=]
      --merge-title-similarity <MERGE_TITLE_SIMILARITY>
          [env: MERGE_TITLE_SIMILARITY=] [default: 0.8]
      --req-limit-count <REQ_LIMIT_COUNT>
          global request count that is per interval [env: REQUEST_LIMIT_COUNT=] [default: 4096]
      --req-limit-interval <REQ_LIMIT_INTERVAL>
          (whole) number of seconds [env: REQUEST_LIMIT_INTERVAL=] [default: 2]
      --per-object-scraper-log-size <PER_OBJECT_SCRAPER_LOG_SIZE>
          Size of the queue keeping track of which scraper touched an object [env: PER_OBJECT_SCRAPER_LOG_SIZE=] [default: 5]
  -h, --help
          Print help
  -V, --version
          Print version
```

# Setup Instructions
## Database Setup
The project currently only works with postgres. Thus, set up a postgres database (or run the docker-compose.yml file included here)  and specify the environment variables as seen in the .env file, with changes as pertaining to your specific database setup.

## Setting up Rust
Set up rustc, cargo etc as described [here](https://rustup.rs/).

## Setting up SQLX
### Installing
Sqlx is the tool we use to set up, manage and connect to the database.
Because sqlx is not only a crate, but also a command line tool to run database setup, migration and all those things, you need to install the tool seperately from the default cargo build process.

To set up sqlx, run `cargo install sqlx_cli --no-default-features --features postgres`. If there is an error like 
```
note: ld: library not found for -lpq
clang: error: linker command failed with exit code 1 (use -v to see invocation)
```
Install the Postgres C libraries and rerun the command. On debian based systems this could for example be: `sudo apt install libpq-dev`.
More information on this process can be found [here](https://sqlx.dev/article/How_to_install_SQLX.html).

### Running Instructions
Setting up the database for development can be done via the sqlx cli tool. Otherwise if the backend detects a database state that was not touched by it it will automatically apply all migrations.

**First, set up the database variables as seen in the docker-compose.yml in the umbrella repository**
Again, in this directory run `sqlx database setup`. This should suffice.
To reset the database to the empty-but-configured state, run `sqlx database reset -f`.

## Building and running the project
First, make sure your cwd is this directory. Then, run `cargo build` to build the project and `cargo run` ro run it.  Easy!

You can configure the backend to either via command line arguments or via environment variables. This  project uses the dotenv crate, so configuring via .env files is very much the way to go if you want to run this in standalone mode.
