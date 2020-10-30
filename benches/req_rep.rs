use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::convert::TryInto;
use std::time::Duration;
use tokio::runtime::Runtime;

use zeromq::{prelude::*, RepSocket, ReqSocket};

type BenchGroup<'a> = criterion::BenchmarkGroup<'a, criterion::measurement::WallTime>;

async fn setup(endpoint: &str) -> (ReqSocket, RepSocket) {
    let mut rep_socket = RepSocket::new();
    let bind_endpoint = rep_socket.bind(endpoint).await.expect("failed to bind rep");
    println!("Bound rep socket to {}", &bind_endpoint);

    let mut req_socket = ReqSocket::new();
    req_socket
        .connect(bind_endpoint)
        .await
        .expect("Failed to connect req");

    (req_socket, rep_socket)
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut rt = Runtime::new().unwrap();

    const N_MSG: u32 = 512;

    let mut group = c.benchmark_group("1-1 Req Rep messaging");

    bench(&mut group, "TCP", "tcp://localhost:0", &mut rt);
    bench(&mut group, "IPC", "ipc://req_rep.sock", &mut rt);

    fn bench(group: &mut BenchGroup, bench_name: &str, endpoint: &str, rt: &mut Runtime) {
        let (req, rep) = rt.block_on(setup(endpoint));
        let (mut req, mut rep) = (Some(req), Some(rep));

        group.bench_function(bench_name, |b| {
            b.iter(|| rt.block_on(iter_fn(&mut req, &mut rep)))
        });
    }

    async fn iter_fn(req: &mut Option<ReqSocket>, rep: &mut Option<RepSocket>) {
        let mut req_owned = req.take().unwrap();
        let mut rep_owned = rep.take().unwrap();
        let rep_handle = tokio::spawn(async move {
            for i in 0..N_MSG {
                let mess: String = rep_owned
                    .recv()
                    .await
                    .expect("Rep failed to receive")
                    .try_into()
                    .unwrap();
                rep_owned
                    .send(format!("{} Rep - {}", mess, i).into())
                    .expect("Rep failed to send");
            }
            // yield for a moment to ensure that server has some time to flush socket
            // tokio::time::delay_for(Duration::from_millis(100)).await;
            rep_owned
        });

        for i in 0..N_MSG {
            req_owned
                .send(format!("Req - {}", i).into())
                .await
                .expect("Req failed to send");
            let repl: String = req_owned
                .recv()
                .await
                .expect("Req failed to recv")
                .try_into()
                .unwrap();
            assert_eq!(format!("Req - {0} Rep - {0}", i), repl);
            black_box(repl);
        }

        let rep_owned = rep_handle.await.expect("Rep task failed");
        req.replace(req_owned);
        rep.replace(rep_owned);
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(128)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(10));
    targets = criterion_benchmark
}
criterion_main!(benches);