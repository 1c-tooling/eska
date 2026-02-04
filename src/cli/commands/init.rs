use crate::error::Result;

#[allow(clippy::unused_async)]
pub async fn run() -> Result<String> {
    Ok("Я комманда init!".to_string())
}
