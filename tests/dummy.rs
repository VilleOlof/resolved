//! Tests that can use the `fudummy` binary instead of `fuscript`, or the real DaVinci Resolve.

mod dummy {
    use std::time::Duration;

    use futures::future::join_all;
    use resolved::prelude::*;
    use tokio::{spawn, time::sleep};

    #[tokio::test]
    async fn simple_lua_execution() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let v = resolve.execute::<i32>("return 1 + 1").await?;
        assert_eq!(2, v);

        Ok(())
    }

    #[tokio::test]
    async fn module_error() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let Error::LuaModuleErr(_) = resolve
            .execute::<()>("<invalid lua syntax>")
            .await
            .err()
            .unwrap()
        else {
            panic!("wrong error type, expected LuaModuleErr")
        };

        Ok(())
    }

    #[tokio::test]
    async fn start_lua_module() -> ResolveResult<()> {
        Resolve::new().await?;
        Ok(())
    }

    #[tokio::test]
    async fn global_resolve() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let exists = resolve.execute::<bool>("return resolve ~= nil").await?;
        assert!(exists);

        Ok(())
    }

    #[tokio::test]
    async fn reset_globals() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        resolve.execute::<()>("my_value = 2").await?;
        let glob: Option<i32> = resolve.execute("return my_value").await?;
        assert!(glob.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn item_reference() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store(r#"return 55"#).await?;
        let value = item.execute::<i32>(r#"return self"#).await?;
        assert_eq!(55, value);

        Ok(())
    }

    #[tokio::test]
    async fn drop_reference() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store("return 3.14").await?;
        assert!(item.execute::<bool>("return self == 3.14").await?);
        let id = item.id();
        drop(item);

        // need to give it time to release it in the module
        sleep(Duration::from_millis(250)).await;

        // fake ref with same id
        let fake_item = unsafe { ItemRef::new(resolve.clone(), id) };

        // error since the id doesnt exist in handler
        let Error::LuaModuleErr(_) = fake_item.execute::<()>("").await.err().unwrap() else {
            panic!("wrong error type, expected LuaModuleErr")
        };

        Ok(())
    }

    #[tokio::test]
    async fn manual_item_drop() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store(r#"return 55"#).await?;
        item.execute::<()>("").await?;
        unsafe { ItemRef::manual_drop(resolve, item.id()).await };

        let Error::LuaModuleErr(_) = item.execute::<()>("").await.err().unwrap() else {
            panic!("wrong error type, expected LuaModuleErr")
        };

        // item's real Drop is called here and will silently fail and print that to stderr

        Ok(())
    }

    #[tokio::test]
    async fn script_arg() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let value: String = resolve
            .execute(Script::new("return arg[1]").arg("Hi!")?)
            .await?;
        assert_eq!("Hi!", value);

        Ok(())
    }

    #[tokio::test]
    async fn script_arg_ref() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store("return 100").await?;

        let value: i32 = resolve
            .execute(Script::new("return arg[1]").arg_ref(&item)?)
            .await?;
        assert_eq!(100, value);

        Ok(())
    }

    #[tokio::test]
    async fn script_named_arg() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let value: f64 = resolve
            .execute(Script::new("return var").named_arg("var", 5.5)?)
            .await?;
        assert_eq!(5.5, value);

        Ok(())
    }

    #[tokio::test]
    async fn script_named_arg_ref() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store("return true").await?;

        let value: bool = resolve
            .execute(Script::new("return var").named_arg_ref("var", &item)?)
            .await?;
        assert_eq!(true, value);

        Ok(())
    }

    #[tokio::test]
    async fn script_many_args() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let mut script = Script::new("return arg[55]");

        for i in 1..=100 {
            script = script.arg(i)?;
        }

        let value: i32 = resolve.execute(script).await?;
        assert_eq!(55, value);

        Ok(())
    }

    #[tokio::test]
    async fn script_self_named() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve
            .store(Script::new("return self").named_arg("self", 5)?)
            .await?;
        // since self is never actually written it shouldnt be 5
        let value: bool = item.execute("return self ~= 5").await?;
        assert!(value);

        Ok(())
    }

    #[tokio::test]
    async fn pooled() -> ResolveResult<()> {
        let resolve = PooledResolve::new(4).await?;

        let value: bool = resolve.execute("return true").await?;
        assert!(value);

        Ok(())
    }

    #[tokio::test]
    async fn pooled_many() -> ResolveResult<()> {
        let pool = PooledResolve::new(4).await?;

        let mut tasks = Vec::with_capacity(64);
        for _ in 0..64 {
            let p = pool.clone();
            tasks.push(spawn(async move { p.execute::<()>("").await.unwrap() }));
        }

        for task in join_all(tasks).await {
            task.unwrap();
        }

        Ok(())
    }

    #[tokio::test]
    async fn resolve_id() -> ResolveResult<()> {
        let a = Resolve::new().await?;
        let b = Resolve::new().await?;
        // its partialeq impl checks their inner ids
        assert!(a != b);
        Ok(())
    }

    #[tokio::test]
    async fn same_resolve() -> ResolveResult<()> {
        let a = Resolve::new().await?;
        let b = a.clone();
        assert!(a == b);
        Ok(())
    }

    #[tokio::test]
    async fn script_different_references() -> Result<(), Error> {
        let a = Resolve::new().await?;
        let b = Resolve::new().await?;

        let a_ref = unsafe { ItemRef::new(a, 1) };
        let b_ref = unsafe { ItemRef::new(b, 1) };

        let mut script = Script::new("return 1");
        script = script.arg_ref(&a_ref)?;

        let Error::MismatchedItemRef(_, _) = b_ref.execute::<i32>(script).await.err().unwrap()
        else {
            panic!("wrong error type, expected MismatchedItemRef")
        };

        Ok(())
    }
}
