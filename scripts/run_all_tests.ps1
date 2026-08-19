# integration tests with a dummy fuscript
./scripts/run_tests_with_dummy.ps1
# lua_module unit tests
./scripts/test_module.ps1
# resolved unit tests
cargo test -- --skip dummy --skip resolve