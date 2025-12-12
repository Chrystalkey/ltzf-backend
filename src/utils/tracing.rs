// TODO:
// 1. console logging according to RUST_LOG (already exists)
// 2. error logging for everything {warn, error}
// 3. a subscriber that catches the (for now) email warnings (`Actionable`)
// 4. a subscriber that logs object creations, deletions and merges

use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::{Layer, registry::LookupSpan};

#[derive(Clone)]
pub struct Logging {
    error_log: PathBuf,
    object_log: Option<PathBuf>,
}

impl Logging {
    pub fn new(error: PathBuf, object: Option<PathBuf>) -> Self {
        Self {
            error_log: error,
            object_log: object,
        }
    }

    pub fn error_layer<S>(&self) -> Box<dyn Layer<S> + Send + Sync + 'static>
    where
        S: tracing::subscriber::Subscriber,
        for<'a> S: LookupSpan<'a>,
    {
        use std::fs;
        let fmt = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_level(true)
            .with_file(true)
            .with_line_number(true);
        if !self
            .error_log
            .parent()
            .expect("File should not be a root node")
            .exists()
        {
            info!("{:?} does not exist, creating...", self.error_log.parent());
            std::fs::create_dir_all(self.error_log.parent().unwrap()).unwrap();
        }

        let file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.error_log)
            .unwrap_or_else(|_| {
                panic!(
                    "Expected to be able to open this file: {:?}",
                    &self.error_log
                )
            });

        fmt.with_writer(file)
            .with_filter(tracing::level_filters::LevelFilter::WARN)
            .boxed()
    }
    pub fn object_log_layer<S>(&self) -> Box<dyn Layer<S> + Send + Sync + 'static>
    where
        S: tracing::subscriber::Subscriber,
        for<'a> S: LookupSpan<'a>,
    {
        use std::fs;
        if let Some(object_log) = &self.object_log {
            let fmt = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_level(true)
                .with_file(true)
                .with_line_number(true);
            if !object_log
                .parent()
                .expect("File should not be a root node")
                .exists()
            {
                info!("{:?} does not exist, creating...", object_log.parent());
                std::fs::create_dir_all(object_log.parent().unwrap()).unwrap();
            }

            let file = fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(object_log)
                .expect("Should Create or Append");

            fmt.with_writer(file)
                .with_filter(tracing_subscriber::filter::filter_fn(|meta| {
                    meta.target() == "obj" && meta.is_event()
                }))
                .boxed()
        } else {
            Box::new(tracing_subscriber::layer::Identity::new())
        }
    }
}
