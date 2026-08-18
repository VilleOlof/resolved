use criterion::{Criterion, criterion_group, criterion_main};
use futures::future::join_all;
use resolved::{OwnedScript, PooledResolve, Resolve, ResolveConfig, Script};
use std::{sync::Arc, time::Duration};

const NOOP: &str = "";
const SLEEP: &str = "sleep(10)";
const LENGTHY: &str = r#"
    local a = 57 + 12
    local b = a + 41
    local c = "aaaa" .. "bbbb"
    local d = a * b
    local e = c .. "cccc"
    function add(a, b)
        return a + b
    end
    local f = add(5, 1)
"#;

async fn run_n<'s>(resolve: &Resolve, n: usize, script: impl Into<Script<'s>>) {
    let script = script.into();
    for _ in 0..n {
        resolve.execute::<()>(script.clone()).await.unwrap();
    }
}

async fn pool_n<'c>(pool: &PooledResolve, n: usize, script: impl Into<OwnedScript>) {
    let mut handles = Vec::with_capacity(n);
    let script = Arc::new(script.into());
    for _ in 0..n {
        let p = pool.clone();
        let script = script.clone();
        handles.push(tokio::spawn(async move {
            p.execute::<()>(script.as_ref()).await.unwrap()
        }));
    }
    join_all(handles).await;
}

// because of how async works and shit, when running this benchmark and the weird runtime / handle behavior
// the background pong task never actually tuns in the Resolve instance
// so we set a really high timeout to avoid the lua module from exiting early
// while still cleaning it up incase the benchmark panics, it will still send a shutdown packet on drop
const BENCHMARK_TIMEOUT: Duration = Duration::from_mins(5);
fn benchmark_config() -> ResolveConfig {
    ResolveConfig {
        timeout: BENCHMARK_TIMEOUT,
        // we dont really run any code, so for performance we dont reset globals
        // this saves a toon of time in smaller tests
        reset_globals: false,
        ..Default::default()
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .name("benchmark")
        .build()
        .unwrap();

    let resolve =
        rt.block_on(async { Resolve::new_with_config(&benchmark_config()).await.unwrap() });
    let pool_2 = rt.block_on(async {
        PooledResolve::new_with_config(2, benchmark_config())
            .await
            .unwrap()
    });
    let pool_8 = rt.block_on(async {
        PooledResolve::new_with_config(8, benchmark_config())
            .await
            .unwrap()
    });

    let mut exec = c.benchmark_group("execute");
    exec.bench_function("single_1", |b| {
        b.to_async(&rt).iter(|| run_n(&resolve, 1, NOOP));
    });
    exec.bench_function("single_64", |b| {
        b.to_async(&rt).iter(|| run_n(&resolve, 64, NOOP));
    });

    exec.bench_function("pool2_1", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_2, 1, NOOP));
    });
    exec.bench_function("pool2_64", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_2, 64, NOOP));
    });

    exec.bench_function("pool8_1", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_8, 1, NOOP));
    });
    exec.bench_function("pool8_64", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_8, 64, NOOP));
    });
    exec.finish();

    let resolve =
        rt.block_on(async { Resolve::new_with_config(&benchmark_config()).await.unwrap() });
    let mut exec_lengthy = c.benchmark_group("execute_lengthy");
    exec_lengthy.bench_function("single_1", |b| {
        b.to_async(&rt).iter(|| run_n(&resolve, 1, LENGTHY));
    });
    exec_lengthy.bench_function("single_64", |b| {
        b.to_async(&rt).iter(|| run_n(&resolve, 64, LENGTHY));
    });
    exec_lengthy.finish();

    let mut work = c.benchmark_group("work");
    work.bench_function("single_1", |b| {
        b.to_async(&rt).iter(|| run_n(&resolve, 1, SLEEP));
    });
    work.bench_function("single_64", |b| {
        b.to_async(&rt).iter(|| run_n(&resolve, 64, SLEEP));
    });

    work.bench_function("pool2_1", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_2, 1, SLEEP));
    });
    work.bench_function("pool2_64", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_2, 64, SLEEP));
    });

    work.bench_function("pool8_1", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_8, 1, SLEEP));
    });
    work.bench_function("pool8_64", |b| {
        b.to_async(&rt).iter(|| pool_n(&pool_8, 64, SLEEP));
    });
    work.finish();

    let mut create = c.benchmark_group("create");
    create.sample_size(50);

    // we push all instances here so the drop impl on the fields dont run until after
    // the drop removes files and does cleaning which slows heavily down the benchmarking by over twice as slow
    // and is not really what we want, we just wanna see times for creation, not dropping
    let instances = Arc::new(tokio::sync::Mutex::new(vec![]));
    create.bench_function("resolve", |b| {
        b.to_async(&rt).iter(|| async {
            let r = Resolve::new_with_config(&benchmark_config()).await.unwrap();
            {
                instances.lock().await.push(r);
            }
        });
    });
    let _ = instances;

    // these spike your cpu to 100% and freezes shit, spooky for now
    // create.bench_function("pool1", |b| {
    //     b.to_async(&rt).iter(|| async {
    //         black_box(PooledResolve::new(1).await.unwrap());
    //     });
    // });
    // create.bench_function("pool4", |b| {
    //     b.to_async(&rt).iter(|| async {
    //         black_box(PooledResolve::new(4).await.unwrap());
    //     });
    // });
    create.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
