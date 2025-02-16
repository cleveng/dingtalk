use deadpool_redis::{Config, Pool, Runtime};

use std::env;
use std::sync::Arc;

mod contact;
mod core;
mod organization;

pub struct DingTalk {
    pub appid: String,
    pub app_secret: String,
    pub client: reqwest::Client,
    pub rdb: Arc<Pool>,
}

impl DingTalk {
    /// Creates a new instance of DingTalk.
    ///
    /// # Arguments
    ///
    /// * `appid` - The app ID issued by DingTalk.
    /// * `app_secret` - The app secret issued by DingTalk.
    pub fn new(appid: String, app_secret: String) -> Self {
        let cfg =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://:@127.0.0.1:6379/1".to_string());

        let pool_config = Config::from_url(cfg);
        let pool = match pool_config.create_pool(Some(Runtime::Tokio1)) {
            Ok(pool) => pool,
            Err(e) => panic!("Failed to create Redis pool: {}", e),
        };

        DingTalk {
            appid,
            app_secret,
            client: reqwest::Client::new(),
            rdb: Arc::new(pool),
        }
    }
}
