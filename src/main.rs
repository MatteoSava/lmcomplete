#[tokio::main]
async fn main() {
    if let Err(error) = lmcomplete::run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
