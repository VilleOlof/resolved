#[cfg(not(windows))]
compile_error!(
    "vinci only works on windows due to dll's and the way the library is structured with lua modules"
);

mod resolve;
mod script;

pub use resolve::Resolve;
pub use script::ScriptResponse;

fn random_port() -> std::io::Result<u16> {
    let l = std::net::TcpListener::bind("0.0.0.0:0")?;
    let p = l.local_addr()?.port();
    drop(l);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test() {
        let resolve = Resolve::new().await;

        let t = std::time::Instant::now();
        let s = resolve
            .deserialize::<String>("return self:GetVersionString()".to_string())
            .await;
        let t = t.elapsed();
        println!("[{:?}]: {s:?}", t);
    }
}
