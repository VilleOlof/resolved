/// Retrieves an available random TCP port assigned by the OS
pub async fn random_port() -> std::io::Result<u16> {
    // TODO: this can randomly panic in race conditions
    let l = tokio::net::TcpListener::bind("0.0.0.0:0").await?;
    let p = l.local_addr()?.port();
    drop(l);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn port() -> std::io::Result<()> {
        let _ = random_port().await?;
        Ok(())
    }
}
