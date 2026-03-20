use crate::services::crawler::CrawlerRequest;
use deadpool_redis::Pool;
use redis::AsyncCommands;

#[derive(Clone)]
pub struct QueueService {
    pool: Pool,
}

impl QueueService {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Push a task to the tail of the queue
    pub async fn enqueue(&self, task: CrawlerRequest) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Redis connection error: {}", e))?;
        let json =
            serde_json::to_string(&task).map_err(|e| format!("Serialization error: {}", e))?;
        let _: () = conn
            .rpush("crawl_queue", json)
            .await
            .map_err(|e| format!("Redis push error: {}", e))?;
        Ok(())
    }

    /// Check Redis connectivity by issuing a PING command.
    pub async fn ping(&self) -> Result<(), String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Redis pool error: {}", e))?;
        let _: String = redis::cmd("PING")
            .query_async(&mut *conn)
            .await
            .map_err(|e| format!("Redis PING error: {}", e))?;
        Ok(())
    }

    /// Block and wait for a task from the head of the queue (timeout in seconds)
    pub async fn dequeue(&self, timeout: f64) -> Result<Option<CrawlerRequest>, String> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| format!("Redis connection error: {}", e))?;

        // blpop returns (key, value)
        let result: Option<(String, String)> = conn
            .blpop("crawl_queue", timeout)
            .await
            .map_err(|e| format!("Redis pop error: {}", e))?;

        match result {
            Some((_key, data)) => {
                let task: CrawlerRequest = serde_json::from_str(&data)
                    .map_err(|e| format!("Deserialization error: {}", e))?;
                Ok(Some(task))
            }
            None => Ok(None),
        }
    }
}
