use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::time::Instant;
use tracing::{instrument, warn};

// the maxmium number of user agents / pathcount entries.
// this is to prevent clogging memory at runtime
const MAX_COUNT: usize = 1024;

#[derive(Debug)]
pub struct Statistics {
    pub pathcounts: RwLock<HashMap<String, u32>>,
    pub useragents: RwLock<HashMap<String, u32>>,
    pub up_since: Instant,
    pub objects_created: RwLock<u32>,
    pub put_with_no_change: RwLock<u32>,
}

impl Statistics {
    pub fn new() -> Self {
        Self {
            pathcounts: HashMap::new().into(),
            useragents: HashMap::new().into(),
            up_since: tokio::time::Instant::now(),
            objects_created: 0.into(),
            put_with_no_change: 0.into(),
        }
    }
    pub async fn obj_created(&self) {
        let mut r = self.objects_created.write().await;
        *r += 1;
    }
    pub async fn put_nochange(&self) {
        let mut r = self.put_with_no_change.write().await;
        *r += 1;
    }
    #[instrument(skip_all, fields(ua=?ua))]
    pub async fn ua_registered(&self, ua: &str) {
        let mut r = self.useragents.write().await;
        if r.len() > MAX_COUNT {
            warn!(
                "Too many unique user agents recorded {MAX_COUNT}, discarding {}",
                ua
            );
            return;
        }
        let ua_cnt: u32 = *r.get(ua).unwrap_or(&0);
        r.insert(ua.to_owned(), ua_cnt + 1);
    }
    #[instrument(skip_all, fields(path=?path))]
    pub async fn path_called(&self, path: &str) {
        let mut r = self.pathcounts.write().await;
        let cnt: u32 = *r.get(path).unwrap_or(&0);
        r.insert(path.to_owned(), cnt + 1);
    }
}
