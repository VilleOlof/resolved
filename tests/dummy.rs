//! Tests that can use the `fudummy` binary instead of `fuscript`, or the real DaVinci Resolve.

mod dummy {
    use std::time::Duration;

    use futures::future::join_all;
    use resolved::{Globals, ResolveConfig, prelude::*};
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

        resolve.execute::<()>("my_value = 2\nreturn nil").await?;
        let glob: Option<i32> = resolve.execute("return my_value").await?;
        assert!(glob.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn config_globals() -> ResolveResult<()> {
        let mut globals = Globals::with_capacity(2);
        globals.add("id", &10)?;
        globals.add("answer", &42)?;

        let resolve = Resolve::new_with_config(&ResolveConfig {
            globals,
            reset_globals: true,
            ..Default::default()
        })
        .await?;

        assert_eq!(10, resolve.execute::<i32>("id").await?);
        assert_eq!(
            42,
            resolve.execute::<i32>("how = false\nreturn answer").await?
        );
        assert_eq!(None, resolve.execute::<Option<bool>>("how").await?);
        assert_eq!(10, resolve.execute::<i32>("id").await?);

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
    async fn reference_value() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store(r#"true"#).await?;
        let value = item.value::<bool>().await?;
        assert!(value);

        Ok(())
    }

    #[tokio::test]
    async fn item_option_reference() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store_option(r#"true"#).await?;
        assert!(item.is_some());

        let item = resolve.store_option(r#"{}"#).await?;
        assert!(item.is_some());

        let item = resolve.store_option(r#"nil"#).await?;
        assert!(item.is_none());

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

        // item's real Drop is called here but vale.dropped will have been set to true so it doesnt run

        Ok(())
    }

    #[tokio::test]
    async fn cloned_reference() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store("10.10").await?;

        // if theres more than one ref to the same underlying id
        // and we drop one, the registry id should remain since theres at least one ref living
        let other = item.clone();
        drop(item);

        // if the first one would have ran the drop packet
        // we wait to be sure:
        sleep(Duration::from_millis(500)).await;

        let value: f64 = other.execute("self").await?;
        assert_eq!(10.10, value);

        Ok(())
    }

    #[tokio::test]
    async fn reference_list() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let items = resolve
            .store_list("{ 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 }")
            .await?;
        assert_eq!(10, items.len());

        for (i, x) in items.list().iter().enumerate() {
            assert_eq!(i + 1, x.value::<usize>().await?);
        }

        Ok(())
    }

    #[tokio::test]
    async fn reference_list_keys() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let map = resolve
            .store_list("{ a = 15, b = 41, c = 95, d = 26, e = 82 }")
            .await?;
        assert_eq!(5, map.len());

        let mut keys = map.keys::<String>().await?;
        keys.sort(); // maps are random order
        assert_eq!(vec!["a", "b", "c", "d", "e"], keys);

        Ok(())
    }

    #[tokio::test]
    async fn drop_reference_list() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let map = resolve.store_list("{ 1, 2, 3, 4 }").await?;
        let ids: Vec<u64> = map.list().iter().map(|x| x.id()).collect();

        drop(map);

        sleep(Duration::from_millis(500)).await;

        for id in ids {
            let item = unsafe { ItemRef::new(resolve.clone(), id) };

            // item id was dropped in the batch drop from the list drop
            // so all of them should return a LuaModuleErr
            let Error::LuaModuleErr(_) = item.value::<i32>().await.err().unwrap() else {
                panic!("wrong error type, expected LuaModuleErr")
            };
        }

        Ok(())
    }

    #[tokio::test]
    async fn script_arg() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let value: String = resolve
            .execute(Script::new("return arg[1]").arg(&"Hi!")?)
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
            .execute(Script::new("return var").named_arg("var", &5.5)?)
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
            script = script.arg(&i)?;
        }

        let value: i32 = resolve.execute(script).await?;
        assert_eq!(55, value);

        Ok(())
    }

    #[tokio::test]
    async fn script_map_arg() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let mut map = std::collections::HashMap::new();
        map.insert("a", 5);
        map.insert("b", 14);

        let script = Script::new(r#"map["a"]"#).named_arg("map", &map)?;

        let value: i32 = resolve.execute(script).await?;
        assert_eq!(5, value);

        Ok(())
    }

    #[tokio::test]
    async fn script_self_named() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve
            .store(Script::new("return self").named_arg("self", &5)?)
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

    #[tokio::test]
    async fn script_no_timeout() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        resolve
            .execute::<()>(Script::new("sleep(1)").with_timeout(Duration::from_millis(100)))
            .await?;

        Ok(())
    }
    #[tokio::test]
    async fn script_timeout() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        let Error::ScriptTimeout(_) = resolve
            .execute::<()>(Script::new("sleep(100)").with_timeout(Duration::from_millis(1)))
            .await
            .err()
            .unwrap()
        else {
            panic!("wrong error type, expected ScriptTimeout")
        };

        Ok(())
    }

    #[tokio::test]
    async fn wrong_handle() -> Result<(), Error> {
        let resolve = Resolve::new().await?;
        // #1 > timeout
        //  #1 sleeps for 1s
        // #2 > send with big timeout
        //  get response from #1
        // expect wronghandle

        let Error::ScriptTimeout(_) = resolve
            .execute::<()>(Script::new("sleep(500)").with_timeout(Duration::from_millis(1)))
            .await
            .err()
            .unwrap()
        else {
            panic!("wrong error type, expected ScriptTimeout")
        };

        let Error::WrongHandle(_, _) = resolve
            .execute::<()>(Script::new("5").with_timeout(Duration::from_secs(10)))
            .await
            .err()
            .unwrap()
        else {
            panic!("wrong error type, expected ScriptTimeout")
        };

        Ok(())
    }

    #[cfg(feature = "macros")]
    mod macros {
        use super::*;

        #[tokio::test]
        async fn macro_simple() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let five = resolve.execute::<i32>(script! { 5 }).await?;
            assert_eq!(5, five);

            let multiline = resolve
                .execute::<i32>(script! {
                    local a = 5
                    local b = 2
                    local c = a * b
                    return c
                })
                .await?;
            assert_eq!(10, multiline);

            Ok(())
        }

        #[tokio::test]
        async fn macro_variables() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let num = 67.69;
            let same: f64 = resolve.execute(script! { $num }).await?;
            assert_eq!(num, same);

            let a = "Hello";
            let b = ", World!";
            let classic: String = resolve.execute(script! { $a .. $b }).await?;
            assert_eq!("Hello, World!", classic);

            let text = "|";
            let many: String = resolve.execute(script! { $text .. $text .. $text }).await?;
            assert_eq!("|||", many);

            Ok(())
        }

        #[tokio::test]
        async fn macro_reference_variables() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let items = resolve.store(script! { { 1, 2, 3, 4, 5 } }).await?;

            let first: i32 = resolve.execute(script! { @items[1] }).await?;
            let last: i32 = resolve.execute(script! { @items[#@items] }).await?;

            assert_eq!(1, first);
            assert_eq!(5, last);

            let both: bool = resolve.execute(script! { $first == @items[1] }).await?;
            assert!(both);

            Ok(())
        }
    }
}
