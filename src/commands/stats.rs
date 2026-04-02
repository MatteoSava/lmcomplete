use anyhow::Result;

use crate::stats::UsageStats;

pub fn run() -> Result<()> {
    let stats = UsageStats::load()?;
    println!("requests: {}", stats.requests);
    println!("prompt_tokens: {}", stats.prompt_tokens);
    println!("completion_tokens: {}", stats.completion_tokens);
    println!("total_tokens: {}", stats.total_tokens);
    println!("total_cost_credits: {:.6}", stats.total_cost_credits);
    if let Some(last_request) = stats.last_request_unix_seconds {
        println!("last_request_unix_seconds: {last_request}");
    }
    Ok(())
}
