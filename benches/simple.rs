use criterion::{Criterion, criterion_group, criterion_main};
use resolved::{PooledResolve, Resolve};
use serde::de::DeserializeOwned;
use std::hint::black_box;
use tokio::runtime::Runtime;

const SLEEP_SCRIPT: &str = r#"local clock = os.clock
function sleep(n)
    local t0 = clock()
    while clock() - t0 <= n do
    end
end

sleep(0.1)
"#;
const VERSION_SCRIPT: &str = "return self:GetVersionString()";

async fn single<T: DeserializeOwned>(amount: usize, script: &'static str) {
    let resolve = Resolve::new().await.unwrap();

    for _ in 0..amount {
        let s = resolve.execute::<T>(script).await.unwrap();
        black_box(s);
    }
}

async fn pool<T: DeserializeOwned + Send + Sync>(amount: usize, pool: usize, script: &'static str) {
    let resolve = PooledResolve::new(pool).await.unwrap();

    let mut handles = Vec::with_capacity(amount);

    for _ in 0..amount {
        let r = resolve.clone();
        handles.push(tokio::task::spawn(async move {
            let s = r.execute::<T>(script).await.unwrap();
            black_box(s);
        }));
    }

    futures::future::join_all(handles).await;
}

fn criterion_benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut version = c.benchmark_group("version");
    version.bench_function("single_1", |b| {
        b.to_async(&rt).iter(|| single::<String>(1, VERSION_SCRIPT))
    });
    version.bench_function("pool_1", |b| {
        b.to_async(&rt)
            .iter(|| pool::<String>(1, 1, VERSION_SCRIPT))
    });
    version.bench_function("single_256", |b| {
        b.to_async(&rt)
            .iter(|| single::<String>(256, VERSION_SCRIPT))
    });
    version.bench_function("pool_256", |b| {
        b.to_async(&rt)
            .iter(|| pool::<String>(256, 2, VERSION_SCRIPT))
    });
    version.finish();

    let mut sleep = c.benchmark_group("sleep");
    sleep.bench_function("single_sleep_16", |b| {
        b.to_async(&rt).iter(|| single::<()>(16, SLEEP_SCRIPT))
    });

    sleep.bench_function("pool_sleep_16", |b| {
        b.to_async(&rt).iter(|| pool::<()>(16, 4, SLEEP_SCRIPT))
    });
    sleep.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
