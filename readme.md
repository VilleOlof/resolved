# resolved

> [!IMPORTANT]  
> This crate only works on **Windows**

## Install

**`tokio`** is required to use `resolved`

```toml
[dependencies]
resolved = "*"
tokio = { version = "*", features = ["full"] }
```

## Usage

```rust ignore
use resolved::{Resolve, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let resolve = Resolve::new().await?;
    let version = resolve.execute::<String>(
        r#"return self:GetVersionString"#
    ).await?;
    Ok(())
}
```
