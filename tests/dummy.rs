//! Tests that can use the `fudummy` binary instead of `fuscript`, or the real DaVinci Resolve.

/// Asserts that an `expr` *(which returns a [`Result`])* matches a `pat`
macro_rules! assert_error {
        ($err:pat = $run:expr, $($arg:tt)+) => {
            let $err = $run.err().expect("Expected an Err(_), got an Ok(_) value") else {
                panic!($($arg)+);
            };
        };
        ($err:pat = $run:expr $(,)?) => {
            assert_error!($err = $run, "Got the wrong error type");
        };
    }

mod dummy {
    use std::time::Duration;

    use futures::future::join_all;
    use resolved::{Globals, ResolveConfig, Void, prelude::*};
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

        assert_error!(
            Error::LuaModuleErr(_) = resolve.execute::<()>("<invalid lua syntax>").await,
            "Expected LuaModuleErr"
        );

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
        assert_error!(
            Error::LuaModuleErr(_) = fake_item.execute::<()>("").await,
            "Expected LuaModuleErr"
        );

        Ok(())
    }

    #[tokio::test]
    async fn manual_item_drop() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let item = resolve.store(r#"return 55"#).await?;
        item.execute::<()>("").await?;
        unsafe { ItemRef::manual_drop(resolve, item.id()).await };

        assert_error!(
            Error::LuaModuleErr(_) = item.execute::<()>("").await,
            "Expected LuaModuleErr"
        );

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
            assert_error!(
                Error::LuaModuleErr(_) = item.value::<i32>().await,
                "Expected LuaModuleErr"
            );
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
    async fn script_remove_named() -> ResolveResult<()> {
        let mut script = Script::new("a + b + c");
        script = script
            .named_arg("a", &5)?
            .named_arg("b", &10)?
            .named_arg("c", &1)?
            .named_arg("d", &9)?;

        assert_eq!(4, script.named_args().len());

        script.remove_named_arg("b");
        script.remove_named_arg("d");

        assert_eq!(2, script.named_args().len());

        script = script.named_arg("b", &1)?;

        assert_eq!(3, script.named_args().len());
        assert_eq!(vec!["a", "c", "b"], script.named_args());

        let resolve = Resolve::new().await?;
        let result = resolve.execute::<i32>(script).await?;
        assert_eq!(7, result);

        Ok(())
    }

    #[tokio::test]
    async fn script_overwrite_named() -> ResolveResult<()> {
        let resolve = Resolve::new().await?;

        let mut script = Script::new("a + a + b");
        script = script.named_arg("a", &5)?.named_arg("b", &2)?;
        assert_eq!(2, script.named_args().len());

        let result_a: i32 = resolve.execute(script.clone()).await?;

        script = script.named_arg("a", &8)?;
        assert_eq!(2, script.named_args().len());

        let result_b: i32 = resolve.execute(script).await?;

        assert_ne!(result_a, result_b);
        assert_eq!(12, result_a);
        assert_eq!(18, result_b);

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

        assert_error!(
            Error::MismatchedItemRef(_, _) = b_ref.execute::<i32>(script).await,
            "Expected MismatchedItemRef"
        );

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

        assert_error!(
            Error::ScriptTimeout(_) = resolve
                .execute::<()>(Script::new("sleep(100)").with_timeout(Duration::from_millis(1)))
                .await,
            "Expected ScriptTimeout"
        );

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

        assert_error!(
            Error::ScriptTimeout(_) = resolve
                .execute::<()>(Script::new("sleep(500)").with_timeout(Duration::from_millis(1)))
                .await,
            "Expected ScriptTimeout"
        );

        assert_error!(
            Error::WrongHandle(_, _) = resolve
                .execute::<()>(Script::new("5").with_timeout(Duration::from_secs(10)))
                .await,
            "Expected WrongHandle"
        );

        Ok(())
    }

    #[tokio::test]
    async fn return_values() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        let int_u8 = resolve.execute::<u8>("5").await?;
        assert_eq!(5u8, int_u8);
        let int_i64 = resolve.execute::<i64>("9191919191919").await?;
        assert_eq!(9191919191919i64, int_i64);
        let float_f32 = resolve.execute::<f32>("14.23").await?;
        assert_eq!(14.23f32, float_f32);

        let string = resolve.execute::<String>(r#""resolved""#).await?;
        assert_eq!("resolved", string);

        let boolean = resolve.execute::<bool>("true").await?;
        assert_eq!(true, boolean);

        let unit = resolve.execute::<()>("nil").await?;
        assert_eq!((), unit);
        let unit = resolve.execute::<()>("").await?;
        assert_eq!((), unit);
        let none = resolve.execute::<Option<()>>("nil").await?;
        assert_eq!(None, none);

        let some = resolve.execute::<Option<i16>>("841").await?;
        assert_eq!(Some(841i16), some);

        // type doesnt matter since this fails in serializing in the first place
        let err_userdata = resolve.execute::<()>("resolve").await;
        assert_error!(Error::LuaModuleErr(_) = err_userdata);

        let userdata = resolve.execute::<Void>("resolve").await?;
        assert_eq!(Void, userdata);

        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct Data {
            name: String,
            age: u8,
            created: i64,
        }

        let custom_struct = resolve
            .execute::<Data>(r#"{ name = "Ben", age = 23, created = 81 }"#)
            .await?;
        assert_eq!(
            Data {
                name: "Ben".to_string(),
                age: 23,
                created: 81
            },
            custom_struct
        );

        Ok(())
    }

    #[tokio::test]
    async fn discord_return_value() -> Result<(), Error> {
        let resolve = Resolve::new().await?;

        // 'self' here is the resolve api, which is userdata that we cant serialize
        // so we throw away it and thus just return `Void`
        let discarded = resolve.execute::<Void>("self").await?;
        assert_eq!(Void, discarded);

        // since we didnt specify `Void`, it will try and serialize it and fail
        // so the type here doesnt matter, could be anything and this would fail
        let userdata = resolve.execute::<()>("self").await;
        assert_error!(Error::LuaModuleErr(_) = userdata);

        Ok(())
    }

    #[cfg(feature = "macros")]
    mod macros {
        use resolved::ToLuaRef;

        use super::*;

        #[tokio::test]
        async fn macro_simple() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let five = resolve.execute::<i32>(script! { 5 }?).await?;
            assert_eq!(5, five);

            let multiline = resolve
                .execute::<i32>(script! {
                    local a = 5
                    local b = 2
                    local c = a * b
                    return c
                }?)
                .await?;
            assert_eq!(10, multiline);

            Ok(())
        }

        #[tokio::test]
        async fn macro_variables() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let num = 67.69;
            let same: f64 = resolve.execute(script! { $num }?).await?;
            assert_eq!(num, same);

            let a = "Hello";
            let b = ", World!";
            let classic: String = resolve.execute(script! { $a .. $b }?).await?;
            assert_eq!("Hello, World!", classic);

            let text = "|";
            let many: String = resolve
                .execute(script! { $text .. $text .. $text }?)
                .await?;
            assert_eq!("|||", many);

            let (a, b, c) = (1, 2, 3);
            let mixed: i32 = resolve.execute(script! { $a + $a + $c * $b / $a }?).await?;
            assert_eq!(8, mixed);

            Ok(())
        }

        #[tokio::test]
        async fn macro_reference_variables() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let items = resolve.store(script! { { 1, 2, 3, 4, 5 } }?).await?;

            let first: i32 = resolve.execute(script! { @items[1] }?).await?;
            let last: i32 = resolve.execute(script! { @items[#@items] }?).await?;

            assert_eq!(1, first);
            assert_eq!(5, last);

            let both: bool = resolve.execute(script! { $first == @items[1] }?).await?;
            assert!(both);

            Ok(())
        }

        #[tokio::test]
        async fn same_name_as_capture() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let my_var = "zz";
            let combined: String = resolve
                .execute(script! {
                    local my_var = "aa"
                    return $my_var .. my_var
                }?)
                .await?;
            assert_eq!("zzaa", combined);

            Ok(())
        }

        #[tokio::test]
        async fn ref_list_as_argument() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let list = [1, 2, 3, 4];
            let list = resolve.store_list(script! { $list }?).await?;

            let len: i32 = resolve
                .execute(script! {
                    return #@list
                }?)
                .await?;
            assert_eq!(4, len);

            Ok(())
        }

        #[tokio::test]
        async fn to_lua_ref_fn() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            async fn add_one(resolve: &Resolve, value: impl ToLuaRef) -> Result<i32, Error> {
                resolve.execute(script! { @value + 1 }?).await
            }

            let six = resolve.store(script! { 6 }?).await?;
            assert_eq!(7, add_one(&resolve, six).await?);

            Ok(())
        }

        #[tokio::test]
        async fn argument_names() -> Result<(), Error> {
            let resolve = Resolve::new().await?;

            let (a, b) = (16, 18);
            let item = resolve.store("1").await?;

            let script = script! { $a + $b + $b + @item }?;
            assert_eq!(vec!["__c0", "__c1", "__r0"], script.named_args());

            Ok(())
        }
    }
}
