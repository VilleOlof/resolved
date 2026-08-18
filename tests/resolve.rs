//! Tests that requires an active connection to resolve, used for fast, real world testing

mod resolve {
    use std::time::Instant;

    use resolved::{ResolveConfig, prelude::*};

    async fn warmup(resolve: &Resolve) -> ResolveResult<()> {
        // just to warmup the client and module a tiny bit
        for _ in 0..10 {
            let _ = resolve
                .execute::<()>("local a = 5 + 5\nlocal b = 1 + 1\nreturn nil")
                .await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn version() -> ResolveResult<()> {
        #[cfg(feature = "tracing")]
        {
            let sub = tracing_subscriber::FmtSubscriber::builder()
                .with_max_level(tracing::Level::TRACE)
                .finish();
            tracing::subscriber::set_global_default(sub).unwrap();
        }

        let resolve = Resolve::new_with_config(&ResolveConfig::keep_globals()).await?;

        warmup(&resolve).await?;

        let time = Instant::now();
        let version: String = resolve
            .execute(Script::new("resolve:GetVersionString()"))
            .await?;
        let elapsed = time.elapsed();

        println!("[{elapsed:?}]: {version}");
        assert!(!version.is_empty());

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        Ok(())
    }
}
