#[cfg(test)]
mod test {
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::{Barrier, Notify};
    use tokio::task::JoinError;
    #[derive(Serialize, Deserialize)]
    struct Foo {
        x: i32,
    }

    #[tokio::test]
    async fn ack1() {
        let mut f = tokio::fs::File::create("/opt/tmp/foo3a.json")
            .await
            .unwrap();
        let b = Foo { x: 5 };
        let v = serde_json::to_vec(&b).unwrap();
        let _ = f.write_all(&v).await;

        async fn post() {
            let mut f = tokio::fs::File::create("/opt/tmp/foo3b.json")
                .await
                .unwrap();
            let b = Foo { x: 5 };
            let v = serde_json::to_vec(&b).unwrap();
            let _ = f.write_all(&v).await;
        }

        let barrier = Arc::new(Barrier::new(2));
        let mut futs = vec![];
        for i in 0..=1 {
            let r1 = barrier.clone();
            let j1 = tokio::spawn(async move {
                let mut f = tokio::fs::File::create(format!("/opt/tmp/foo{i}.json"))
                    .await
                    .unwrap();
                tokio::time::sleep(Duration::from_millis((2 - i) * 1000)).await;
                let b = Foo { x: 5 };
                let v = serde_json::to_vec(&b).unwrap();
                let _ = f.write_all(&v).await.unwrap();
                let _ = f.sync_all().await.unwrap();
                if r1.wait().await.is_leader() {
                    let _ = post().await;
                }
            });
            futs.push(j1);
        }

        for f in futs {
            let _ = f.await;
        }
    }

    #[tokio::test]
    async fn ack() {
        let barrier = Arc::new(Barrier::new(2));
        let n = Arc::new(Notify::new());
        let n1 = n.clone();
        let r1 = barrier.clone();
        let j1 = tokio::spawn(async move {
            let mut f = tokio::fs::File::create("/opt/tmp/foo1.json").await.unwrap();
            // tokio::time::sleep(Duration::from_millis(3000)).await;
            let b = Foo { x: 5 };
            let v = serde_json::to_vec(&b).unwrap();
            let _ = f.write_all(&v).await.unwrap();
            let _ = f.sync_all().await.unwrap();
            if r1.wait().await.is_leader() {
                n1.notify_one();
            }
        });
        let n2 = n.clone();
        let r2 = barrier.clone();
        let j2 = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(2000)).await;
            let mut f = tokio::fs::File::create("/opt/tmp/foo2.json").await.unwrap();
            let b = Foo { x: 5 };
            let v = serde_json::to_vec(&b).unwrap();
            let _ = f.write_all(&v).await.unwrap();
            if r2.wait().await.is_leader() {
                n2.notify_one();
            }
        });
        let master = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let mut f = tokio::fs::File::create("/opt/tmp/foo3a.json")
                .await
                .unwrap();
            let b = Foo { x: 5 };
            let v = serde_json::to_vec(&b).unwrap();
            let _ = f.write_all(&v).await;

            let _ = n.notified().await;

            let mut f = tokio::fs::File::create("/opt/tmp/foo3b.json")
                .await
                .unwrap();
            let b = Foo { x: 5 };
            let v = serde_json::to_vec(&b).unwrap();
            let _ = f.write_all(&v).await;
        });
        let (_, _, _) = tokio::join!(j1, j2, master);
    }

    #[tokio::test]
    async fn foo_join() -> Result<(), u32> {
        let v1 = tokio::task::spawn(async {
            let Ok(mut f) = tokio::fs::File::open("/opt/tmp/foo.json").await else {
                println!("v1 file bad");
                return Ok(Foo { x: 0 });
            };
            let mut v = vec![];
            let Ok(_r) = f.read_to_end(&mut v).await else {
                println!("v1 read bad");
                return Ok(Foo { x: 0 });
            };
            let Ok(mut foo) = serde_json::from_slice::<Foo>(&v) else {
                println!("v1 json bad");
                return Ok(Foo { x: 0 });
            };
            foo.x *= 2;
            tokio::io::Result::Ok(foo)
        });
        let v2 = tokio::task::spawn(async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let Ok(mut f) = tokio::fs::File::open("/opt/tmp/foo.json").await else {
                println!("v2 file bad");
                return Err(tokio::io::Error::last_os_error());
            };
            let mut v = vec![];
            let Ok(_r) = f.read_to_end(&mut v).await else {
                println!("v2 read bad");
                return Ok(Foo { x: 0 });
            };
            let Ok(mut foo) = serde_json::from_slice::<Foo>(&v) else {
                println!("v2 json bad");
                return Ok(Foo { x: 0 });
            };
            foo.x *= 3;
            tokio::io::Result::Ok(foo)
        });
        macro_rules! l1 {
            ($r:expr) => {
                $r.map_or(-5, |r1| r1.map_or(-6, |f| f.x))
            };
        }
        let l = |r: Result<Result<Foo, tokio::io::Error>, JoinError>| {
            r.map_or(-5, |r1| r1.map_or(-6, |f| f.x))
        };
        let (a, b) = tokio::join!(v1, v2);
        let a = l(a);
        let b = l1!(b);
        println!("a={a} b={b}");
        Ok(())
    }

    #[tokio::test]
    async fn foo1() -> Result<(), u32> {
        let v1 = tokio::task::spawn(async {
            let Ok(mut f) = tokio::fs::File::open("/opt/tmp/foo.json").await else {
                println!("v1 file bad");
                return Ok(Foo { x: 0 });
            };
            let mut v = vec![];
            let Ok(_r) = f.read_to_end(&mut v).await else {
                println!("v1 read bad");
                return Ok(Foo { x: 0 });
            };
            let Ok(mut foo) = serde_json::from_slice::<Foo>(&v) else {
                println!("v1 json bad");
                return Ok(Foo { x: 0 });
            };
            foo.x *= 2;
            tokio::io::Result::Ok(foo)
        });
        let v2 = tokio::task::spawn(async {
            let Ok(mut f) = tokio::fs::File::open("/opt/tmp/foo1.json").await else {
                println!("v2 file bad");
                return Err(tokio::io::Error::last_os_error());
            };
            let mut v = vec![];
            let Ok(_r) = f.read_to_end(&mut v).await else {
                println!("v2 read bad");
                return Ok(Foo { x: 0 });
            };
            let Ok(mut foo) = serde_json::from_slice::<Foo>(&v) else {
                println!("v2 json bad");
                return Ok(Foo { x: 0 });
            };
            foo.x *= 3;
            tokio::io::Result::Ok(foo)
        });
        let mut a = None;
        let mut b = None;
        tokio::select! {
            biased;
            Ok(Ok(r))= v1, if a.is_none()=> { a=Some(r.x) }
            Ok(Ok(r))= v2, if b.is_none()=> { b=Some(r.x) }
        };
        let a = a.unwrap_or(-4);
        let b = b.unwrap_or(-3);
        println!("a={a} b={b}");
        Ok(())
    }

    #[tokio::test]
    async fn foo() -> Result<(), u32> {
        let v1 = tokio::task::spawn(async {
            let Ok(mut f) = tokio::fs::File::open("/opt/tmp/foo.json").await else {
                println!("v1 file bad");
                return Ok(Foo { x: 0 });
            };
            let mut v = vec![];
            let Ok(_r) = f.read_to_end(&mut v).await else {
                println!("v1 read bad");
                return Ok(Foo { x: 0 });
            };
            let Ok(mut foo) = serde_json::from_slice::<Foo>(&v) else {
                println!("v1 json bad");
                return Ok(Foo { x: 0 });
            };
            foo.x *= 2;
            tokio::io::Result::Ok(foo)
        });
        let v2 = tokio::task::spawn(async {
            let Ok(mut f) = tokio::fs::File::open("/opt/tmp/foo1.json").await else {
                println!("v2 file bad");
                return Err(tokio::io::Error::last_os_error());
            };
            let mut v = vec![];
            let Ok(_r) = f.read_to_end(&mut v).await else {
                println!("v2 read bad");
                return Ok(Foo { x: 0 });
            };
            let Ok(mut foo) = serde_json::from_slice::<Foo>(&v) else {
                println!("v2 json bad");
                return Ok(Foo { x: 0 });
            };
            foo.x *= 2;
            tokio::io::Result::Ok(foo)
        });
        let y = tokio::select! {
            Ok(Ok(r))= v1=> { r.x }
            Ok(Ok(r))= v2=> { r.x*2 }
            // else => { -1 }
        };
        println!("y={y}");
        Ok(())
    }
}
