# LTZF-Backend
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