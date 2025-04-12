# Setting up the Environment for developing this project
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