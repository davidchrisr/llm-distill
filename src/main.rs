#[tokio::main]
async fn main() {
    if let Err(error) = llm_distill::run().await {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
